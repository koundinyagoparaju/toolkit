use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};
use x509_cert::certificate::CertificateInner;
use x509_cert::der::oid::ObjectIdentifier;
use x509_cert::der::DecodePem;
use x509_cert::der::{self, Decode};

/// Decode an X.509 certificate (PEM or DER) into its facts — subject,
/// issuer, validity, SANs, algorithms — the jwt-decode of certificates.
/// People paste certs into websites exactly the way they paste JWTs;
/// this keeps them on the device. Reports, never validates: expiry is a
/// judgment for the caller and their clock, not a pure function.
pub struct CertDecode;

const EXAMPLE: &str = "-----BEGIN CERTIFICATE-----
MIIB1jCCAXygAwIBAgIUJRpz1He9M5PrqZXCYUrvhkOhWCMwCgYIKoZIzj0EAwIw
LDEUMBIGA1UEAwwLZXhhbXBsZS5jb20xFDASBgNVBAoMC0V4YW1wbGUgT3JnMB4X
DTI2MDcxMzIwMDczMVoXDTM2MDcxMDIwMDczMVowLDEUMBIGA1UEAwwLZXhhbXBs
ZS5jb20xFDASBgNVBAoMC0V4YW1wbGUgT3JnMFkwEwYHKoZIzj0CAQYIKoZIzj0D
AQcDQgAEmU81u4n0LEVmArE/KW1W431hJMJDwzeuvPnwp2ICP1G6WGlxC6qxxLH7
5R510RrUm1Kgp2D+PDV75qrq6nDykaN8MHowHQYDVR0OBBYEFG6IU6PJoBTv3EIw
ugiyd7NgUALBMB8GA1UdIwQYMBaAFG6IU6PJoBTv3EIwugiyd7NgUALBMA8GA1Ud
EwEB/wQFMAMBAf8wJwYDVR0RBCAwHoILZXhhbXBsZS5jb22CD3d3dy5leGFtcGxl
LmNvbTAKBggqhkjOPQQDAgNIADBFAiEA32V4fPyP5Ataz6xKgkV7dk17nv8O6L2i
qFFShmutwLsCID/KWgtU+jETt39uNViDfXphj7J8mO8BNHI4VC8c9xoN
-----END CERTIFICATE-----";

impl Tool for CertDecode {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "cert-decode".into(),
            label: "Certificate Decode".into(),
            description: "Decode an X.509 certificate (PEM or DER) into subject, issuer, \
                          serial, validity window, subject alternative names, and algorithms — \
                          without pasting it into someone's website. Reports facts only; \
                          checking expiry against \"now\" is left to you."
                .into(),
            keywords: [
                "certificate",
                "x509",
                "tls",
                "ssl",
                "pem",
                "der",
                "decode",
                "san",
                "expiry",
                "inspect",
            ]
            .map(String::from)
            .to_vec(),
            inputs: vec![InputSpec::named(InputSpec::SOLE_NAME, DataType::Bytes)
                .describe("A certificate, PEM text or raw DER bytes.")
                .example(EXAMPLE)],
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Bytes(bytes) = inputs.sole() else {
            unreachable!()
        };
        let text = String::from_utf8_lossy(&bytes);
        let cert = if text.contains("-----BEGIN") {
            CertificateInner::from_pem(text.trim().as_bytes())
                .map_err(|e| ToolError::new(format!("not a valid PEM certificate: {e}")))?
        } else {
            CertificateInner::from_der(&bytes)
                .map_err(|e| ToolError::new(format!("not a valid DER certificate: {e}")))?
        };

        let tbs = cert.tbs_certificate();
        let validity = tbs.validity();
        let mut out = serde_json::json!({
            "subject": tbs.subject().to_string(),
            "issuer": tbs.issuer().to_string(),
            "serial": tbs.serial_number().to_string(),
            "not_before": validity.not_before.to_string(),
            "not_after": validity.not_after.to_string(),
            "signature_algorithm": algorithm_name(&cert.signature_algorithm().oid),
            "public_key_algorithm": algorithm_name(
                &tbs.subject_public_key_info().algorithm.oid
            ),
            "self_signed": tbs.subject() == tbs.issuer(),
        });

        for ext in tbs.extensions().iter().flat_map(|v| v.iter()) {
            match ext.extn_id.to_string().as_str() {
                // subjectAltName: a SEQUENCE of GeneralNames; report the
                // dNSName/iPAddress entries people actually look for.
                "2.5.29.17" => {
                    out["subject_alternative_names"] =
                        serde_json::json!(san_entries(ext.extn_value.as_bytes()));
                }
                // basicConstraints: first field is the CA boolean.
                "2.5.29.19" => {
                    let is_ca = ext.extn_value.as_bytes().len() > 2;
                    out["is_ca"] = serde_json::json!(is_ca);
                }
                _ => {}
            }
        }
        Ok(DataValue::Json(out))
    }
}

/// dNSName (context tag 2) and iPAddress (tag 7) entries from a DER
/// SubjectAltName SEQUENCE, decoded manually: the entries are
/// IMPLICIT-tagged strings, simple enough to walk without the full
/// GeneralName machinery.
fn san_entries(der_bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(seq) = der::asn1::AnyRef::from_der(der_bytes) else {
        return names;
    };
    let Ok(mut reader) = der::SliceReader::new(seq.value()) else {
        return names;
    };
    use x509_cert::der::Reader;
    while !reader.is_finished() {
        let Ok(any) = reader.decode::<der::asn1::AnyRef>() else {
            break;
        };
        use x509_cert::der::Tagged;
        let tag_number = any.tag().number().value();
        match tag_number {
            2 => {
                if let Ok(s) = core::str::from_utf8(any.value()) {
                    names.push(format!("DNS:{s}"));
                }
            }
            7 => {
                let ip = any.value();
                if ip.len() == 4 {
                    names.push(format!("IP:{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]));
                } else {
                    names.push(format!("IP:{}", hex(ip)));
                }
            }
            _ => {}
        }
    }
    names
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Friendly names for the OIDs that appear on real certificates.
fn algorithm_name(oid: &ObjectIdentifier) -> String {
    match oid.to_string().as_str() {
        "1.2.840.113549.1.1.11" => "RSA with SHA-256".into(),
        "1.2.840.113549.1.1.12" => "RSA with SHA-384".into(),
        "1.2.840.113549.1.1.13" => "RSA with SHA-512".into(),
        "1.2.840.113549.1.1.1" => "RSA".into(),
        "1.2.840.10045.4.3.2" => "ECDSA with SHA-256".into(),
        "1.2.840.10045.4.3.3" => "ECDSA with SHA-384".into(),
        "1.2.840.10045.2.1" => "Elliptic curve".into(),
        "1.3.101.112" => "Ed25519".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn decode(input: &[u8]) -> Result<serde_json::Value, ToolError> {
        run_single(
            &CertDecode,
            DataValue::Bytes(input.to_vec()),
            &Options::new(),
        )
        .map(|v| {
            let DataValue::Json(v) = v else {
                unreachable!()
            };
            v
        })
    }

    #[test]
    fn decodes_the_example_pem() {
        let v = decode(EXAMPLE.as_bytes()).unwrap();
        assert_eq!(v["subject"], "O=Example Org,CN=example.com");
        assert_eq!(v["self_signed"], true);
        assert_eq!(v["signature_algorithm"], "ECDSA with SHA-256");
        assert_eq!(v["public_key_algorithm"], "Elliptic curve");
        assert_eq!(
            v["subject_alternative_names"],
            serde_json::json!(["DNS:example.com", "DNS:www.example.com"])
        );
        assert_eq!(v["is_ca"], true);
        assert!(v["not_after"].as_str().unwrap().starts_with("2036"));
    }

    #[test]
    fn junk_errors_cleanly() {
        assert!(decode(b"not a certificate").is_err());
        assert!(
            decode(b"-----BEGIN CERTIFICATE-----\ngarbage\n-----END CERTIFICATE-----").is_err()
        );
    }
}
