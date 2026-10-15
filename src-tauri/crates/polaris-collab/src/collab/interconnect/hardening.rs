//! Typed ingress and background delivery policy; old configurations inherit defaults.
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ingress {
    pub trusted_proxy_cidrs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailDelivery {
    /// Total admitted jobs, including those currently running.
    pub capacity: usize,
    pub concurrency: usize,
    pub timeout_secs: u64,
}

impl MailDelivery {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1024).contains(&self.capacity) || !(1..=64).contains(&self.concurrency)
            || self.concurrency > self.capacity || !(1..=300).contains(&self.timeout_secs)
        {
            return Err("mailDelivery: capacity=1..1024, concurrency=1..64 且不超过 capacity, timeoutSecs=1..300".into());
        }
        Ok(())
    }
}

/// No new dependency: CIDR membership is a masked integer comparison.
pub struct Cidr {
    address: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let invalid = || "ingress.trustedProxyCidrs 须为 IPv4/IPv6 CIDR".to_string();
        let (address, prefix) = raw.split_once('/').ok_or_else(invalid)?;
        let address: IpAddr = address.parse().map_err(|_| invalid())?;
        let prefix: u8 = prefix.parse().map_err(|_| invalid())?;
        if prefix > if address.is_ipv4() { 32 } else { 128 } {
            return Err(invalid());
        }
        Ok(Self { address, prefix })
    }

    pub fn contains(&self, address: IpAddr) -> bool {
        let address = normalize(address);
        match (self.address, address) {
            (IpAddr::V4(network), IpAddr::V4(ip)) => {
                let mask = u32::MAX.checked_shl(32 - u32::from(self.prefix)).unwrap_or(0);
                u32::from(network) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(ip)) => {
                let mask = u128::MAX.checked_shl(128 - u32::from(self.prefix)).unwrap_or(0);
                u128::from(network) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

pub fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        _ => ip,
    }
}

impl Ingress {
    pub fn networks(&self) -> Result<Vec<Cidr>, String> {
        self.trusted_proxy_cidrs.iter().map(|s| Cidr::parse(s)).collect()
    }

    pub fn validate(&self) -> Result<(), String> {
        self.networks().map(|_| ())
    }
}
