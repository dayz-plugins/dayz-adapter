//! `dayz.connect`'s argument: a host name or address with an optional port.

/// The port DayZ servers listen on unless they say otherwise.
pub(crate) const DEFAULT_PORT: u16 = 2302;

/// Parse `host[:port]`. An `IPv6` address has to be bracketed to carry a port, as in a URL.
///
/// # Errors
/// Empty input, or a port that is not a number from 1 to 65535.
pub(crate) fn parse(text: &str) -> Result<(String, u16), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("connect to what? give a host name or address, with :port if not 2302".into());
    }
    let (host, port) = if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| format!("{text:?} opens a bracket it does not close"))?;
        (host, after.strip_prefix(':'))
    } else if text.matches(':').count() == 1 {
        let (host, port) = text.split_once(':').unwrap_or((text, ""));
        (host, Some(port))
    } else {
        (text, None)
    };
    let port = match port {
        None => DEFAULT_PORT,
        Some(digits) => digits
            .parse::<u16>()
            .ok()
            .filter(|p| *p != 0)
            .ok_or_else(|| format!("{digits:?} is not a port"))?,
    };
    if host.is_empty() {
        return Err(format!("{text:?} has a port but no host"));
    }
    Ok((host.to_owned(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_defaults_and_can_be_given() {
        assert_eq!(parse("127.0.0.1"), Ok(("127.0.0.1".into(), 2302)));
        assert_eq!(
            parse(" example.org:2402 "),
            Ok(("example.org".into(), 2402))
        );
        assert_eq!(parse("[::1]:2302"), Ok(("::1".into(), 2302)));
        assert_eq!(parse("::1"), Ok(("::1".into(), 2302)));
    }

    #[test]
    fn nonsense_is_refused() {
        assert!(parse("").is_err());
        assert!(parse("host:port").is_err());
        assert!(parse("host:0").is_err());
        assert!(parse(":2302").is_err());
        assert!(parse("[::1").is_err());
    }
}
