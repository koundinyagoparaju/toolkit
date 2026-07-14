use std::net::{Ipv4Addr, Ipv6Addr};
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, OptGet, OptionSpec, Options, Tool,
    ToolError,
};

/// CIDR arithmetic: network/broadcast/host range/mask from a prefix, and
/// an optional does-this-block-contain-that-IP check. IPv4 and IPv6,
/// pure integer math (std's address types parse; no sockets anywhere).
pub struct CidrCalc;

impl Tool for CidrCalc {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "cidr-calc".into(),
            label: "CIDR Calculator".into(),
            description: "Expand a CIDR block (v4 or v6) into network, broadcast/last address, \
                          host range, count, and mask — with an optional contains-IP check."
                .into(),
            keywords: [
                "cidr", "subnet", "ip", "network", "mask", "prefix", "ipv4", "ipv6", "range",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "10.0.0.0/22"),
            output: DataType::Json,
            streaming: false,
            options: vec![OptionSpec::string(
                "contains",
                "Contains IP",
                "An address to test for membership in the block. Empty skips the check.",
            )
            .default_value("".into())],
        }
    }

    fn run(&self, inputs: Inputs, options: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let text = text.trim();
        let (addr, prefix) = match text.split_once('/') {
            Some((a, p)) => (
                a.trim(),
                Some(
                    p.trim()
                        .parse::<u8>()
                        .map_err(|_| ToolError::new(format!("\"{p}\" is not a prefix length")))?,
                ),
            ),
            None => (text, None),
        };
        let contains = options.str_opt("contains").unwrap_or("").trim().to_string();

        let mut out = if let Ok(v4) = addr.parse::<Ipv4Addr>() {
            v4_block(v4, prefix.unwrap_or(32), &contains)?
        } else if let Ok(v6) = addr.parse::<Ipv6Addr>() {
            v6_block(v6, prefix.unwrap_or(128), &contains)?
        } else {
            return Err(ToolError::new(format!(
                "\"{addr}\" is not an IPv4 or IPv6 address"
            )));
        };
        out["input"] = serde_json::json!(text);
        Ok(DataValue::Json(out))
    }
}

fn v4_block(addr: Ipv4Addr, prefix: u8, contains: &str) -> Result<serde_json::Value, ToolError> {
    if prefix > 32 {
        return Err(ToolError::new("IPv4 prefix must be 0-32"));
    }
    let bits = u32::from(addr);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    let network = bits & mask;
    let broadcast = network | !mask;
    // /31 and /32 have no network/broadcast split (RFC 3021).
    let (first, last, hosts) = if prefix >= 31 {
        (network, broadcast, u64::from(broadcast - network) + 1)
    } else {
        (
            network + 1,
            broadcast - 1,
            u64::from(broadcast - network) - 1,
        )
    };
    let mut out = serde_json::json!({
        "version": 4,
        "network": Ipv4Addr::from(network).to_string(),
        "broadcast": Ipv4Addr::from(broadcast).to_string(),
        "first_host": Ipv4Addr::from(first).to_string(),
        "last_host": Ipv4Addr::from(last).to_string(),
        "hosts": hosts,
        "mask": Ipv4Addr::from(mask).to_string(),
        "prefix": prefix,
    });
    if !contains.is_empty() {
        let ip: Ipv4Addr = contains
            .parse()
            .map_err(|_| ToolError::new(format!("\"{contains}\" is not an IPv4 address")))?;
        out["contains"] = serde_json::json!(u32::from(ip) & mask == network);
        out["contains_ip"] = serde_json::json!(contains);
    }
    Ok(out)
}

fn v6_block(addr: Ipv6Addr, prefix: u8, contains: &str) -> Result<serde_json::Value, ToolError> {
    if prefix > 128 {
        return Err(ToolError::new("IPv6 prefix must be 0-128"));
    }
    let bits = u128::from(addr);
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    let network = bits & mask;
    let last = network | !mask;
    let count = if prefix == 0 {
        "340282366920938463463374607431768211456".to_string()
    } else {
        (1u128 << (128 - prefix)).to_string()
    };
    let mut out = serde_json::json!({
        "version": 6,
        "network": Ipv6Addr::from(network).to_string(),
        "last": Ipv6Addr::from(last).to_string(),
        "addresses": count,
        "prefix": prefix,
    });
    if !contains.is_empty() {
        let ip: Ipv6Addr = contains
            .parse()
            .map_err(|_| ToolError::new(format!("\"{contains}\" is not an IPv6 address")))?;
        out["contains"] = serde_json::json!(u128::from(ip) & mask == network);
        out["contains_ip"] = serde_json::json!(contains);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn calc(input: &str, contains: &str) -> Result<serde_json::Value, ToolError> {
        let mut opts = Options::new();
        opts.insert("contains".into(), contains.into());
        run_single(&CidrCalc, DataValue::Text(input.into()), &opts).map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    #[test]
    fn v4_blocks() {
        let v = calc("10.0.0.0/22", "").unwrap();
        assert_eq!(v["network"], "10.0.0.0");
        assert_eq!(v["broadcast"], "10.0.3.255");
        assert_eq!(v["first_host"], "10.0.0.1");
        assert_eq!(v["hosts"], 1022);
        assert_eq!(v["mask"], "255.255.252.0");

        // Non-aligned address normalizes to its network.
        assert_eq!(
            calc("192.168.1.130/25", "").unwrap()["network"],
            "192.168.1.128"
        );
        // /31 point-to-point and /32 host routes.
        assert_eq!(calc("10.0.0.0/31", "").unwrap()["hosts"], 2);
        assert_eq!(calc("10.0.0.7/32", "").unwrap()["hosts"], 1);
        assert_eq!(calc("10.0.0.7", "").unwrap()["prefix"], 32);

        // Textbook /26: 192.168.100.14 sits in the .0-.63 block.
        let v = calc("192.168.100.14/26", "").unwrap();
        assert_eq!(v["network"], "192.168.100.0");
        assert_eq!(v["broadcast"], "192.168.100.63");
        assert_eq!(v["last_host"], "192.168.100.62");
        assert_eq!(v["hosts"], 62);
        assert_eq!(v["mask"], "255.255.255.192");

        // /0 covers everything without shifting by the full word width.
        let v = calc("0.0.0.0/0", "").unwrap();
        assert_eq!(v["broadcast"], "255.255.255.255");
        assert_eq!(v["mask"], "0.0.0.0");
        assert_eq!(calc("0.0.0.0/0", "203.0.113.9").unwrap()["contains"], true);
    }

    #[test]
    fn v6_blocks_and_contains() {
        let v = calc("2001:db8::/48", "").unwrap();
        assert_eq!(v["network"], "2001:db8::");
        assert_eq!(v["last"], "2001:db8:0:ffff:ffff:ffff:ffff:ffff");

        assert_eq!(calc("10.0.0.0/22", "10.0.3.7").unwrap()["contains"], true);
        assert_eq!(calc("10.0.0.0/22", "10.0.4.1").unwrap()["contains"], false);
        assert_eq!(
            calc("2001:db8::/48", "2001:db8:0:1::5").unwrap()["contains"],
            true
        );
    }

    #[test]
    fn junk_errors() {
        assert!(calc("not-an-ip/8", "").is_err());
        assert!(calc("10.0.0.0/33", "").is_err());
        assert!(calc("10.0.0.0/22", "nope").is_err());
    }
}
