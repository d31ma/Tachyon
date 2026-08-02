use crate::Failure;
use crate::failure::diagnostic;
use serde::Serialize;
use tachyon_contracts::{
    HandlerBody, HandlerCancel, HandlerHeaders, HandlerRequest, HandlerResponse,
};

pub(super) const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
pub(super) const FRAME_PREFIX_BYTES: usize = 4;

pub(super) fn request_frame(request: &HandlerRequest) -> Result<Vec<u8>, Failure> {
    validate_request(request)?;
    encode_frame(request)
}

pub(super) fn cancel_frame(request_id: &str) -> Result<Vec<u8>, Failure> {
    encode_frame(&HandlerCancel::v1(request_id))
}

pub(crate) fn response_frame(bytes: &[u8], request_id: &str) -> Result<HandlerResponse, Failure> {
    if bytes.len() < FRAME_PREFIX_BYTES {
        return Err(protocol_failure(
            2102,
            "Handler stdout ended before a frame prefix was received.",
        ));
    }
    let length = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(protocol_failure(
            2103,
            "Handler response frame exceeds the 16 MiB protocol limit.",
        ));
    }
    let expected = FRAME_PREFIX_BYTES.saturating_add(length);
    if bytes.len() != expected {
        let message = if bytes.len() > expected {
            "Handler wrote trailing or multiple data to protocol stdout."
        } else {
            "Handler stdout ended before the declared frame was complete."
        };
        return Err(protocol_failure(2102, message));
    }
    let Ok(json) = std::str::from_utf8(&bytes[FRAME_PREFIX_BYTES..]) else {
        return Err(protocol_failure(
            2102,
            "Handler response frame is not valid UTF-8.",
        ));
    };
    let Ok(response) = serde_json::from_str::<HandlerResponse>(json) else {
        return Err(protocol_failure(
            2102,
            "Handler response is not a valid Handler Protocol v1 response.",
        ));
    };
    validate_response(&response, request_id)?;
    Ok(response)
}

fn encode_frame<T: Serialize>(envelope: &T) -> Result<Vec<u8>, Failure> {
    let Ok(json) = serde_json::to_vec(envelope) else {
        return Err(Failure::one(diagnostic(
            2005,
            "Handler request cannot be serialized.",
            Some(String::from(
                "Use Handler Protocol v1-compatible request values.",
            )),
            None,
        )));
    };
    if json.len() > MAX_FRAME_BYTES {
        return Err(Failure::one(diagnostic(
            2005,
            "Handler request frame exceeds the 16 MiB protocol limit.",
            Some(String::from("Reduce request headers or body size.")),
            None,
        )));
    }
    let Ok(length) = u32::try_from(json.len()) else {
        return Err(Failure::one(diagnostic(
            2005,
            "Handler request frame length cannot be represented.",
            None,
            None,
        )));
    };
    let mut frame = Vec::with_capacity(FRAME_PREFIX_BYTES + json.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&json);
    Ok(frame)
}

fn validate_request(request: &HandlerRequest) -> Result<(), Failure> {
    if request.protocol_version != 1
        || !valid_identifier(&request.request_id)
        || !valid_operation(&request.operation)
        || !valid_route(&request.route)
        || request
            .deadline_ms
            .is_some_and(|deadline| !(1..=300_000).contains(&deadline))
        || !valid_headers(&request.headers)
        || request.body.as_ref().is_some_and(|body| !valid_body(body))
    {
        return Err(Failure::one(diagnostic(
            2005,
            "Handler request does not satisfy Handler Protocol v1.",
            Some(String::from(
                "Check the request ID, operation, route, deadline, headers, and body limits.",
            )),
            None,
        )));
    }
    Ok(())
}

fn validate_response(response: &HandlerResponse, request_id: &str) -> Result<(), Failure> {
    let valid_error = response.error.as_ref().is_none_or(|error| {
        valid_code(&error.code)
            && !error.message.is_empty()
            && error.message.chars().count() <= 2_048
    });
    if response.protocol_version != 1
        || response.request_id != request_id
        || !valid_identifier(&response.request_id)
        || !(100..=599).contains(&response.status)
        || !valid_headers(&response.headers)
        || response.body.as_ref().is_some_and(|body| !valid_body(body))
        || response.body.is_some() == response.error.is_some()
        || !valid_error
    {
        return Err(protocol_failure(
            2102,
            "Handler response violates Handler Protocol v1 or does not match the request.",
        ));
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_operation(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && value.len() <= 128
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        })
}

fn valid_route(value: &str) -> bool {
    value.starts_with('/') && value.chars().count() <= 2_048
}

fn valid_headers(headers: &HandlerHeaders) -> bool {
    headers.len() <= 128
        && headers.iter().all(|(name, values)| {
            valid_header_name(name)
                && values.len() <= 64
                && values.iter().all(|value| value.chars().count() <= 8_192)
        })
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn valid_body(body: &HandlerBody) -> bool {
    body.data.chars().count() <= MAX_FRAME_BYTES
}

fn valid_code(code: &str) -> bool {
    code.len() == 6 && code.starts_with("TY") && code[2..].bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn protocol_failure(number: u16, message: &str) -> Failure {
    Failure::one(diagnostic(
        number,
        message,
        Some(String::from(
            "Fix or replace the handler adapter before retrying.",
        )),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::{MAX_FRAME_BYTES, cancel_frame, request_frame, response_frame};
    use tachyon_contracts::{
        HandlerBody, HandlerBodyEncoding, HandlerHeaders, HandlerRequest, HandlerResponse,
        HttpMethod,
    };

    #[test]
    fn frames_round_trip_and_reject_trailing_or_mismatched_output() {
        let request = HandlerRequest::route("req_1", "/", HttpMethod::Get);
        let request_bytes = request_frame(&request).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            u32::from_be_bytes([
                request_bytes[0],
                request_bytes[1],
                request_bytes[2],
                request_bytes[3]
            ]) as usize,
            request_bytes.len() - 4
        );

        let response = HandlerResponse::success(
            "req_1",
            200,
            HandlerHeaders::new(),
            HandlerBody {
                encoding: HandlerBodyEncoding::Utf8,
                data: String::from("ok"),
            },
        );
        let json = serde_json::to_vec(&response).unwrap_or_else(|_| unreachable!());
        let length = u32::try_from(json.len()).unwrap_or_else(|_| unreachable!());
        let mut frame = Vec::from(length.to_be_bytes());
        frame.extend_from_slice(&json);
        assert_eq!(
            response_frame(&frame, "req_1").unwrap_or_else(|_| unreachable!()),
            response
        );
        frame.push(0);
        assert!(response_frame(&frame, "req_1").is_err());
        frame.pop();
        assert!(response_frame(&frame, "other").is_err());
    }

    #[test]
    fn invalid_requests_and_frames_fail_closed() {
        let mut request = HandlerRequest::route("", "missing-slash", HttpMethod::Get);
        request.deadline_ms = Some(0);
        assert!(request_frame(&request).is_err());

        assert!(response_frame(&[], "req").is_err());
        assert!(response_frame(&[0, 0, 0, 4, b'{', b'}'], "req").is_err());
        assert!(response_frame(&[0, 0, 0, 3, 0xff, 0xff, 0xff], "req").is_err());
        assert!(response_frame(&[0, 0, 0, 3, b'x', b'x', b'x'], "req").is_err());
        let oversized = u32::try_from(MAX_FRAME_BYTES + 1)
            .unwrap_or_else(|_| unreachable!())
            .to_be_bytes();
        assert!(response_frame(&oversized, "req").is_err());
        assert!(cancel_frame(&"x".repeat(MAX_FRAME_BYTES + 1)).is_err());
    }

    #[test]
    fn invalid_response_fields_are_rejected_after_deserialization() {
        let baseline = HandlerResponse::success(
            "req",
            200,
            HandlerHeaders::new(),
            HandlerBody {
                encoding: HandlerBodyEncoding::Utf8,
                data: String::from("ok"),
            },
        );
        let mut invalid = Vec::new();
        let mut protocol = baseline.clone();
        protocol.protocol_version = 2;
        invalid.push(protocol);
        let mut status = baseline.clone();
        status.status = 99;
        invalid.push(status);
        let mut header = baseline.clone();
        header
            .headers
            .insert(String::from("bad header"), vec![String::new()]);
        invalid.push(header);
        let mut neither = baseline;
        neither.body = None;
        invalid.push(neither);

        for response in invalid {
            let json = serde_json::to_vec(&response).unwrap_or_else(|_| unreachable!());
            let length = u32::try_from(json.len()).unwrap_or_else(|_| unreachable!());
            let mut frame = Vec::from(length.to_be_bytes());
            frame.extend_from_slice(&json);
            assert!(response_frame(&frame, "req").is_err());
        }
    }
}
