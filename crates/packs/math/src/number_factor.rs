use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Exact integer anatomy: prime factorization and primality for each
/// number, gcd/lcm across them. Miller–Rabin (deterministic for u64) +
/// Pollard's rho, so large semiprimes factor quickly too.
pub struct NumberFactor;

impl Tool for NumberFactor {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "number-factor".into(),
            label: "Number Factor".into(),
            description: "Prime factorization and primality of each integer in the input, plus \
                          gcd and lcm when there are several. Exact, up to 2^64-1."
                .into(),
            keywords: [
                "factor",
                "prime",
                "factorization",
                "gcd",
                "lcm",
                "divisor",
                "integer",
                "primality",
            ]
            .map(String::from)
            .to_vec(),
            inputs: InputSpec::sole_example(DataType::Text, "360 84"),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let mut numbers = Vec::new();
        for raw in text.split(|c: char| c.is_whitespace() || c == ',') {
            if raw.is_empty() {
                continue;
            }
            let n: u64 = raw.parse().map_err(|_| {
                ToolError::new(format!("\"{raw}\" is not a positive integer up to 2^64-1"))
            })?;
            if n == 0 {
                return Err(ToolError::new("0 has no prime factorization"));
            }
            numbers.push(n);
        }
        if numbers.is_empty() {
            return Err(ToolError::new("no integers in the input"));
        }

        let entries: Vec<serde_json::Value> = numbers
            .iter()
            .map(|&n| {
                let factors = factorize(n);
                let text = factors
                    .iter()
                    .map(|(p, e)| {
                        if *e == 1 {
                            p.to_string()
                        } else {
                            format!("{p}^{e}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" * ");
                serde_json::json!({
                    "n": n,
                    "is_prime": n > 1 && factors.len() == 1 && factors[0].1 == 1,
                    "factors": factors.iter().map(|(p, e)| serde_json::json!({"prime": p, "exp": e})).collect::<Vec<_>>(),
                    "factorization": if n == 1 { "1".into() } else { text },
                })
            })
            .collect();

        let mut out = serde_json::json!({ "numbers": entries });
        if numbers.len() > 1 {
            let g = numbers.iter().copied().reduce(gcd).expect("non-empty");
            out["gcd"] = serde_json::json!(g);
            let mut l: u128 = 1;
            for &n in &numbers {
                let g = gcd_u128(l, n as u128);
                match (l / g).checked_mul(n as u128) {
                    Some(v) => l = v,
                    None => {
                        return Err(ToolError::new("the lcm overflows 128 bits"));
                    }
                }
            }
            out["lcm"] = serde_json::json!(l.to_string());
        }
        Ok(DataValue::Json(out))
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

fn gcd_u128(a: u128, b: u128) -> u128 {
    if b == 0 {
        a
    } else {
        gcd_u128(b, a % b)
    }
}

/// Sorted (prime, exponent) pairs. Trial division for small factors,
/// then Miller–Rabin + Pollard's rho for what remains.
fn factorize(n: u64) -> Vec<(u64, u32)> {
    let mut n = n;
    let mut primes = Vec::new();
    for p in [2u64, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31] {
        while n.is_multiple_of(p) {
            primes.push(p);
            n /= p;
        }
    }
    if n > 1 {
        let mut stack = vec![n];
        while let Some(m) = stack.pop() {
            if is_prime(m) {
                primes.push(m);
            } else {
                let d = pollard_rho(m);
                stack.push(d);
                stack.push(m / d);
            }
        }
    }
    primes.sort_unstable();
    let mut pairs: Vec<(u64, u32)> = Vec::new();
    for p in primes {
        match pairs.last_mut() {
            Some((q, e)) if *q == p => *e += 1,
            _ => pairs.push((p, 1)),
        }
    }
    pairs
}

fn mul_mod(a: u64, b: u64, m: u64) -> u64 {
    ((a as u128 * b as u128) % m as u128) as u64
}

fn pow_mod(mut base: u64, mut exp: u64, m: u64) -> u64 {
    let mut acc = 1u64;
    base %= m;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = mul_mod(acc, base, m);
        }
        base = mul_mod(base, base, m);
        exp >>= 1;
    }
    acc
}

/// Deterministic Miller–Rabin for u64 (these witnesses cover the range).
fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    for p in [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        if n == p {
            return true;
        }
        if n.is_multiple_of(p) {
            return false;
        }
    }
    let mut d = n - 1;
    let mut r = 0;
    while d.is_multiple_of(2) {
        d /= 2;
        r += 1;
    }
    'witness: for a in [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37] {
        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 0..r - 1 {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'witness;
            }
        }
        return false;
    }
    true
}

/// Pollard's rho with Floyd cycle detection. `n` must be odd, composite,
/// and free of the small primes trial division removed.
fn pollard_rho(n: u64) -> u64 {
    for c in 1.. {
        let f = |x: u64| (mul_mod(x, x, n) + c) % n;
        let (mut x, mut y, mut d) = (2u64, 2u64, 1u64);
        while d == 1 {
            x = f(x);
            y = f(f(y));
            d = gcd(x.abs_diff(y), n);
        }
        if d != n {
            return d;
        }
    }
    unreachable!("some cycle constant always splits a composite")
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn factor(input: &str) -> serde_json::Value {
        let DataValue::Json(v) = run_single(
            &NumberFactor,
            DataValue::Text(input.into()),
            &Options::new(),
        )
        .unwrap() else {
            unreachable!()
        };
        v
    }

    #[test]
    fn factorization_gcd_lcm() {
        let v = factor("360 84");
        assert_eq!(v["numbers"][0]["factorization"], "2^3 * 3^2 * 5");
        assert_eq!(v["numbers"][1]["factorization"], "2^2 * 3 * 7");
        assert_eq!(v["gcd"], 12);
        assert_eq!(v["lcm"], "2520");

        assert_eq!(factor("1")["numbers"][0]["factorization"], "1");
        assert_eq!(factor("97")["numbers"][0]["is_prime"], true);
    }

    #[test]
    fn large_values() {
        // Largest u64 prime.
        let v = factor("18446744073709551557");
        assert_eq!(v["numbers"][0]["is_prime"], true);

        // A 63-bit semiprime of two 10-digit primes (needs rho, not
        // trial division): 2147483647 (M31) * 2147483629.
        let v = factor("4611685975477714963");
        assert_eq!(v["numbers"][0]["is_prime"], false);
        assert_eq!(v["numbers"][0]["factorization"], "2147483629 * 2147483647");

        // Perfect power.
        let v = factor("1073741824");
        assert_eq!(v["numbers"][0]["factorization"], "2^30");
    }

    #[test]
    fn junk_errors() {
        for bad in ["", "0", "-4", "2.5", "abc", "18446744073709551616"] {
            assert!(
                run_single(&NumberFactor, DataValue::Text(bad.into()), &Options::new()).is_err(),
                "{bad}"
            );
        }
    }
}
