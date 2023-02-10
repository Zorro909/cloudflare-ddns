use crate::cloudflare::cache::{Cache, DnsRecord};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::from_str;

pub mod cache;

static API_URL: &str = "https://api.cloudflare.com/client/v4";

#[derive(Serialize, Deserialize)]
pub struct DomainRegistration {
    pub domain: String,
    pub v4_disabled: bool,
    pub v4_suffix: Option<String>,
    pub v6_disabled: bool,
    pub v6_suffix: Option<String>,
}

impl DomainRegistration {
    pub fn new(
        domain: &String,
        disable_v4: &bool,
        v4_suffix: &Option<String>,
        disable_v6: &bool,
        v6_suffix: &Option<String>,
    ) -> DomainRegistration {
        DomainRegistration {
            domain: domain.clone(),
            v4_disabled: *disable_v4,
            v4_suffix: v4_suffix.clone(),
            v6_disabled: *disable_v6,
            v6_suffix: v6_suffix.clone(),
        }
    }
}

#[derive(Deserialize)]
struct CloudflareApiResponse<V> {
    success: bool,
    errors: Vec<String>,
    result: Option<V>,
}

#[derive(Deserialize)]
struct CloudflareZone {
    id: String,
}

#[derive(Deserialize)]
struct CloudflareDnsRecord {
    id: String,
    name: String,
    content: String,
    #[serde(rename = "type")]
    record_type: String,
}

pub(crate) struct CloudflareApi {
    token: String,
    client: Client,
    cache: Cache,
}

impl CloudflareApi {
    pub fn new(token: String) -> CloudflareApi {
        CloudflareApi {
            token,
            client: Client::new(),
            cache: Cache::new(),
        }
    }

    pub fn fetch_cloudflare_zones(self: &mut Self) -> Result<Vec<String>, String> {
        // Fetch all zones from Cloudflare API or return cached response
        if !self.cache.zones_cached() {
            let api_response: Result<Vec<CloudflareZone>, String> =
                self.fetch_cloudflare_api("zones".to_string());
            let zones = match api_response {
                Ok(zones) => zones,
                Err(e) => return Err(e),
            };

            zones
                .iter()
                .for_each(|zone| self.cache.add_zone(zone.id.clone()));
        }
        Ok(self.cache.get_zones())
    }

    pub fn fetch_cloudflare_dns_record<'c>(
        self: &'c mut Self,
        domain: &str,
        record_type: &str,
    ) -> Result<&'c DnsRecord, String> {
        // Fetch all dns records for a given zone from Cloudflare API or return cached response
        if self.cache.get_dns_record(domain, record_type).is_none() {
            let zones = match self.fetch_cloudflare_zones() {
                Ok(zones) => zones,
                Err(e) => return Err(e),
            };

            for zone in zones.iter() {
                let dns_records: Vec<CloudflareDnsRecord> = match self
                    .fetch_cloudflare_api(format!("zones/{}/dns_records?type=A&type=AAAA", zone))
                {
                    Ok(dns_records) => dns_records,
                    Err(e) => return Err(e),
                };

                for record in dns_records.iter() {
                    self.cache.set_dns_record(
                        record.name.as_str(),
                        record.record_type.as_str(),
                        record.id.as_str(),
                        zone,
                        record.content.as_str(),
                    );
                }
            }
        }
        self.cache
            .get_dns_record(domain, record_type)
            .ok_or("Unable to find record".to_string())
    }

    pub fn update_cloudflare_dns_record<'c>(
        self: &'c mut Self,
        domain: &str,
        record_type: &str,
        content: &str,
    ) -> Result<&'c DnsRecord, String> {
        // Update a dns record for a given zone from Cloudflare API
        let body = format!("{{\"content\": \"{}\"}}", content);

        let record = match self.fetch_cloudflare_dns_record(domain, record_type) {
            Ok(record) => record.clone(),
            Err(cause) => {
                return Err(format!(
                    "Unable to find record for {} {} (Cause: {})",
                    domain, record_type, cause
                ));
            }
        };

        let api_response: Result<CloudflareDnsRecord, String> = self.put_cloudflare_api(
            format!("zones/{}/dns_records/{}", record.zone_id, record.id),
            body,
        );

        let new_ip = match api_response {
            Ok(api_response) => api_response.content,
            Err(e) => return Err(e),
        };

        if new_ip != content {
            return Err(format!(
                "Unable to update dns record in Cloudflare API: Record not updated"
            ));
        }

        self.cache.set_dns_record(
            domain,
            record_type,
            record.id.as_str(),
            record.zone_id.as_str(),
            new_ip.as_str(),
        );
        self.cache
            .get_dns_record(domain, record_type)
            .ok_or("Unable to fetch updated IP from Cloudflare API".to_string())
    }

    fn fetch_cloudflare_api<V: for<'a> Deserialize<'a>>(
        self: &Self,
        path: String,
    ) -> Result<V, String> {
        // Make Request to Cloudflare API with the given path and return the result as json
        let url = format!("{}/{}", API_URL, path);
        let authorization_header = format!("Bearer {}", self.token);

        self.client
            .get(url)
            .header("Authorization", authorization_header)
            .send()
            .and_then(|res| res.text())
            .map_err(|e| e.to_string())
            .and_then(|body| from_str(&body).map_err(|e| e.to_string()))
            .and_then(
                |api_response: CloudflareApiResponse<V>| match api_response.success {
                    true => Ok(api_response.result.unwrap()),
                    false => Err(format!(
                        "Error in get request to Cloudflare API: {:?}",
                        api_response.errors
                    )),
                },
            )
    }

    fn put_cloudflare_api<V: for<'a> Deserialize<'a>>(
        self: &Self,
        path: String,
        body: String,
    ) -> Result<V, String> {
        // Make Request to Cloudflare API with the given path and return the result as json
        let url = format!("{}/{}", API_URL, path);
        let authorization_header = format!("Bearer {}", self.token);

        self.client
            .put(url)
            .header("Authorization", authorization_header)
            .body(body)
            .send()
            .and_then(|res| res.text())
            .map_err(|e| e.to_string())
            .and_then(|body| from_str(body.as_str()).map_err(|e| e.to_string()))
            .and_then(
                |api_response: CloudflareApiResponse<V>| match api_response.success {
                    true => Ok(api_response.result.unwrap()),
                    false => Err(format!(
                        "Error in put request to Cloudflare API: {:?}",
                        api_response.errors
                    )),
                },
            )
    }
}
