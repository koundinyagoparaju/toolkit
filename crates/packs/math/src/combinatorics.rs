use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Exact combinatorics: n! and, given k, C(n,k) and P(n,k). A tiny
/// hand-rolled bignum (base-1e9 limbs) keeps the results exact where
/// u128 and f64 both give up.
pub struct Combinatorics;

const MAX_N: u64 = 5000;

impl Tool for Combinatorics {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "combinatorics".into(),
            label: "Combinatorics".into(),
            description: "Exact factorial, combinations, and permutations. Input \"n\" gives n!; \
                          \"n k\" adds C(n,k) (choose) and P(n,k) (arrangements). Results are \
                          exact arbitrary-precision integers, n up to 5000."
                .into(),
            keywords: [
                "combinatorics",
                "factorial",
                "ncr",
                "npr",
                "choose",
                "binomial",
                "permutation",
                "combination",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "52 5"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let parts: Vec<&str> = text.split_whitespace().collect();
        let parse = |s: &str| -> Result<u64, ToolError> {
            let n: u64 = s
                .parse()
                .map_err(|_| ToolError::new(format!("\"{s}\" is not a non-negative integer")))?;
            if n > MAX_N {
                return Err(ToolError::new(format!("n is capped at {MAX_N}")));
            }
            Ok(n)
        };
        let (n, k) = match parts.as_slice() {
            [n] => (parse(n)?, None),
            [n, k] => (parse(n)?, Some(parse(k)?)),
            _ => return Err(ToolError::new("input must be \"n\" or \"n k\"")),
        };

        let mut out = serde_json::json!({
            "n": n,
            "factorial": factorial(n).to_string(),
        });
        if let Some(k) = k {
            if k > n {
                return Err(ToolError::new(format!("k ({k}) must not exceed n ({n})")));
            }
            out["k"] = serde_json::json!(k);
            out["combinations"] = serde_json::json!(choose(n, k).to_string());
            out["permutations"] = serde_json::json!(permutations(n, k).to_string());
        }
        Ok(DataValue::Json(out))
    }
}

/// Non-negative bignum, base-1e9 limbs, least significant first. Only
/// what factorials need: multiply and exactly divide by a small number.
struct Big(Vec<u64>);

const BASE: u64 = 1_000_000_000;

impl Big {
    fn one() -> Big {
        Big(vec![1])
    }

    fn mul_small(&mut self, m: u64) {
        let mut carry = 0u64;
        for limb in &mut self.0 {
            let v = *limb * m + carry;
            *limb = v % BASE;
            carry = v / BASE;
        }
        while carry > 0 {
            self.0.push(carry % BASE);
            carry /= BASE;
        }
    }

    /// Exact division by a small divisor (callers guarantee it divides).
    fn div_small_exact(&mut self, d: u64) {
        let mut rem = 0u64;
        for limb in self.0.iter_mut().rev() {
            let v = rem * BASE + *limb;
            *limb = v / d;
            rem = v % d;
        }
        debug_assert_eq!(rem, 0, "division was not exact");
        while self.0.len() > 1 && *self.0.last().expect("non-empty") == 0 {
            self.0.pop();
        }
    }
}

impl std::fmt::Display for Big {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.last().expect("non-empty"))?;
        for limb in self.0.iter().rev().skip(1) {
            write!(f, "{limb:09}")?;
        }
        Ok(())
    }
}

fn factorial(n: u64) -> Big {
    let mut acc = Big::one();
    for i in 2..=n {
        acc.mul_small(i);
    }
    acc
}

fn permutations(n: u64, k: u64) -> Big {
    let mut acc = Big::one();
    for i in (n - k + 1)..=n {
        acc.mul_small(i);
    }
    acc
}

/// Multiplicative formula; each step's division is exact because
/// C(n-k+i, i) is an integer.
fn choose(n: u64, k: u64) -> Big {
    let k = k.min(n - k);
    let mut acc = Big::one();
    for i in 1..=k {
        acc.mul_small(n - k + i);
        acc.div_small_exact(i);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn comb(input: &str) -> Result<serde_json::Value, ToolError> {
        run_single(
            &Combinatorics,
            DataValue::Text(input.into()),
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
    fn known_values() {
        let v = comb("52 5").unwrap();
        assert_eq!(v["combinations"], "2598960"); // poker hands
        assert_eq!(v["permutations"], "311875200");

        assert_eq!(comb("0").unwrap()["factorial"], "1");
        assert_eq!(comb("10").unwrap()["factorial"], "3628800");
        assert_eq!(comb("10 3").unwrap()["permutations"], "720");
        assert_eq!(comb("5 0").unwrap()["combinations"], "1");
        assert_eq!(comb("5 5").unwrap()["combinations"], "1");

        // 100! is 158 digits with a known head and 24 trailing zeros.
        let f = comb("100").unwrap()["factorial"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(f.len(), 158);
        assert!(f.starts_with("93326215443944152681"));
        assert!(f.ends_with("000000000000000000000000"));

        // C(1000, 500) is 300 digits (a bignum-only value).
        let c = comb("1000 500").unwrap()["combinations"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(c.len(), 300);
        assert!(c.starts_with("27028824094543656951"));
    }

    #[test]
    fn junk_errors() {
        for bad in ["", "5 6", "-1", "2.5", "1 2 3", "5001"] {
            assert!(comb(bad).is_err(), "{bad}");
        }
    }
}
