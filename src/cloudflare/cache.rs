use std::collections::HashMap;

pub struct Cache {
    zones: Vec<String>,
    dns_records: HashMap<String, DnsRecord>,
}

#[derive(Clone)]
pub struct DnsRecord {
    pub id: String,
    pub zone_id: String,
    pub content: String,
}

impl Cache {
    pub fn new() -> Self {
        Self {
            zones: Vec::new(),
            dns_records: HashMap::new(),
        }
    }

    pub fn zones_cached(&self) -> bool {
        !self.zones.is_empty()
    }

    pub fn get_zones(&self) -> Vec<String> {
        self.zones.clone()
    }

    pub fn get_dns_record(&self, domain: &str, record_type: &str) -> Option<&DnsRecord> {
        self.dns_records.get(format!("{}_{}", record_type, domain).as_str())
    }

    pub fn add_zone(&mut self, zone_id: String) {
        self.zones.push(zone_id);
    }

    pub fn set_dns_record(&mut self, domain: &str, record_type: &str, record_id: &str, zone_id: &str, content: &str) {
        self.dns_records.insert(format!("{}_{}", record_type, domain), DnsRecord {
            id: record_id.to_string(),
            zone_id: zone_id.to_string(),
            content: content.to_string(),
        });
    }
}