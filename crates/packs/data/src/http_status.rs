use serde_json::json;
use toolkit_core::{
    DataType, DataValue, InputSpec, Inputs, InputsExt, Manifest, Options, Tool, ToolError,
};

/// Look up an HTTP status code's meaning — the "what's a 418 again?" tool,
/// entirely offline.
pub struct HttpStatus;

impl Tool for HttpStatus {
    fn manifest(&self) -> Manifest {
        Manifest {
            name: "http-status".into(),
            label: "HTTP Status".into(),
            description: "Explain an HTTP status code: its name, category, and meaning. Give just the number (extra text is ignored).".into(),
            keywords: ["http", "status", "code", "response", "rest", "api"]
                .map(String::from)
                .to_vec(),
            inputs: InputSpec::sole(DataType::Text),
            output: DataType::Json,
            streaming: false,
            options: vec![],
        }
    }

    fn run(&self, inputs: Inputs, _: &Options) -> Result<DataValue, ToolError> {
        let DataValue::Text(text) = inputs.sole() else {
            unreachable!()
        };
        let code: u16 = text
            .split(|c: char| !c.is_ascii_digit())
            .find(|s| !s.is_empty())
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| ToolError::new("no status code found in the input"))?;

        let (name, description) = lookup(code)
            .ok_or_else(|| ToolError::new(format!("{code} is not a known HTTP status code")))?;
        Ok(DataValue::Json(json!({
            "code": code,
            "name": name,
            "category": category(code),
            "description": description,
        })))
    }
}

fn category(code: u16) -> &'static str {
    match code / 100 {
        1 => "Informational",
        2 => "Success",
        3 => "Redirection",
        4 => "Client Error",
        5 => "Server Error",
        _ => "Unknown",
    }
}

fn lookup(code: u16) -> Option<(&'static str, &'static str)> {
    Some(match code {
        100 => ("Continue", "The client should continue with its request."),
        101 => (
            "Switching Protocols",
            "The server is switching protocols per the Upgrade header.",
        ),
        103 => ("Early Hints", "Preload hints before the final response."),
        200 => ("OK", "The request succeeded."),
        201 => (
            "Created",
            "The request succeeded and a new resource was created.",
        ),
        202 => (
            "Accepted",
            "The request was accepted but not yet processed.",
        ),
        204 => ("No Content", "Success with no response body."),
        206 => (
            "Partial Content",
            "The server is delivering part of the resource (range request).",
        ),
        301 => (
            "Moved Permanently",
            "The resource has permanently moved to a new URL.",
        ),
        302 => ("Found", "The resource is temporarily at a different URL."),
        303 => (
            "See Other",
            "Retrieve the resource with a GET at another URL.",
        ),
        304 => ("Not Modified", "The cached copy is still valid."),
        307 => (
            "Temporary Redirect",
            "Temporary redirect; keep the original method.",
        ),
        308 => (
            "Permanent Redirect",
            "Permanent redirect; keep the original method.",
        ),
        400 => (
            "Bad Request",
            "The server could not understand the request (malformed syntax).",
        ),
        401 => (
            "Unauthorized",
            "Authentication is required and has failed or not been provided.",
        ),
        402 => (
            "Payment Required",
            "Reserved for future use; sometimes used for paid APIs.",
        ),
        403 => (
            "Forbidden",
            "The server understood the request but refuses to authorize it.",
        ),
        404 => ("Not Found", "The requested resource does not exist."),
        405 => (
            "Method Not Allowed",
            "The HTTP method is not supported for this resource.",
        ),
        406 => (
            "Not Acceptable",
            "No representation matches the Accept headers.",
        ),
        408 => (
            "Request Timeout",
            "The server timed out waiting for the request.",
        ),
        409 => (
            "Conflict",
            "The request conflicts with the current state of the resource.",
        ),
        410 => ("Gone", "The resource is permanently gone."),
        411 => ("Length Required", "A Content-Length header is required."),
        413 => (
            "Payload Too Large",
            "The request body is larger than the server will accept.",
        ),
        414 => (
            "URI Too Long",
            "The request URI is longer than the server will accept.",
        ),
        415 => (
            "Unsupported Media Type",
            "The request's media type is unsupported.",
        ),
        418 => (
            "I'm a teapot",
            "The server refuses to brew coffee with a teapot (RFC 2324).",
        ),
        422 => (
            "Unprocessable Entity",
            "The request was well-formed but semantically invalid.",
        ),
        425 => (
            "Too Early",
            "The server is unwilling to risk processing a replayed request.",
        ),
        426 => (
            "Upgrade Required",
            "The client should switch to a different protocol.",
        ),
        428 => ("Precondition Required", "The request must be conditional."),
        429 => (
            "Too Many Requests",
            "The client has sent too many requests (rate limited).",
        ),
        431 => (
            "Request Header Fields Too Large",
            "The headers are too large to process.",
        ),
        451 => (
            "Unavailable For Legal Reasons",
            "The resource is blocked for legal reasons.",
        ),
        500 => (
            "Internal Server Error",
            "The server encountered an unexpected condition.",
        ),
        501 => (
            "Not Implemented",
            "The server does not support the functionality required.",
        ),
        502 => (
            "Bad Gateway",
            "An upstream server returned an invalid response.",
        ),
        503 => (
            "Service Unavailable",
            "The server is overloaded or down for maintenance.",
        ),
        504 => (
            "Gateway Timeout",
            "An upstream server did not respond in time.",
        ),
        505 => (
            "HTTP Version Not Supported",
            "The HTTP version is not supported.",
        ),
        511 => (
            "Network Authentication Required",
            "The client must authenticate to gain network access.",
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use toolkit_core::run_single;

    fn info(input: &str) -> serde_json::Value {
        let DataValue::Json(v) =
            run_single(&HttpStatus, DataValue::Text(input.into()), &Options::new()).unwrap()
        else {
            unreachable!()
        };
        v
    }

    #[test]
    fn known_codes() {
        assert_eq!(info("404")["name"], "Not Found");
        assert_eq!(info("404")["category"], "Client Error");
        assert_eq!(info("418")["name"], "I'm a teapot");
        // Extra text around the number is fine.
        assert_eq!(info("HTTP 200 OK")["code"], 200);
    }

    #[test]
    fn unknown_and_junk_error() {
        assert!(run_single(&HttpStatus, DataValue::Text("299".into()), &Options::new()).is_err());
        assert!(run_single(&HttpStatus, DataValue::Text("nope".into()), &Options::new()).is_err());
    }
}
