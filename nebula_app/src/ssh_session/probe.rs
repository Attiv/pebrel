//! `ssh -G` 探测目标规范化：历史 `user@host:port`、URI 用户名编码和端口拆分，
//! 生成不会改变 Host 匹配或 Match user 语义的命令行参数。

use std::borrow::Cow;
use std::io;

/// 给 `ssh -G` 的离线配置探测目标。Nebula 历史存盘格式允许
/// `user@host:port`，而 OpenSSH 只会从 `ssh://user@host:port` URI 中拆出
/// 端口。URI 用户名需编码内部 @ 和字面 %；original 仍是未经编码的身份。
/// 无显式端口的裸地址保持原样，避免改变 Host 匹配。
pub(crate) fn ssh_config_probe_target(value: &str) -> Cow<'_, str> {
    let uri = value.starts_with("ssh://");
    let address = value.strip_prefix("ssh://").unwrap_or(value);
    let (username, host_port) = address
        .rsplit_once('@')
        .map_or((None, address), |(username, host_port)| (Some(username), host_port));
    let has_explicit_port = if let Some(rest) = host_port.strip_prefix('[') {
        rest.split_once(']')
            .and_then(|(_, suffix)| suffix.strip_prefix(':'))
            .is_some_and(|port| port.parse::<u16>().is_ok())
    } else if let Some((host, port)) = host_port.rsplit_once(':') {
        !host.is_empty() && !host.contains(':') && port.parse::<u16>().is_ok()
    } else {
        false
    };

    if !uri && !has_explicit_port {
        return Cow::Borrowed(value);
    }
    if let Some(username) = username {
        let encoded = encode_ssh_uri_username(username);
        if !uri || encoded != username {
            return Cow::Owned(format!("ssh://{encoded}@{host_port}"));
        }
    }
    if uri { Cow::Borrowed(value) } else { Cow::Owned(format!("ssh://{value}")) }
}

fn encode_ssh_uri_username(username: &str) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(username.len());
    for byte in username.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            // Encode raw UTF-8 bytes once, including %, rather than decoding
            // existing escapes: the application's username is literal text.
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

/// Some system OpenSSH versions reject bracketed IPv6 URIs and non-ASCII
/// command-line users. A literal User option preserves Match user and %r on
/// clients accepting those bytes; newer native validation may still reject
/// them. Expansion-bearing users keep their existing args.
pub(crate) fn ssh_config_probe_args(value: &str) -> Vec<String> {
    let address = value.strip_prefix("ssh://").unwrap_or(value);
    let (username, host_port) = address
        .rsplit_once('@')
        .map_or((None, address), |(username, host_port)| (Some(username), host_port));
    let literal_user = username.is_some_and(|user| {
        !user.is_ascii()
            && !user.contains('%')
            && !user.contains("${")
            && !user.starts_with('-')
            && !user.ends_with('\\')
            && !user.contains(['(', ')', '{', '}'])
            && crate::ssh_profiles::validate_ssh_destination(value).is_ok()
    });
    if let Ok((host, port)) = parse_host_port_optional(host_port)
        && (host.parse::<std::net::Ipv6Addr>().is_ok() || literal_user)
    {
        let mut args = Vec::new();
        if let Some(username) = username {
            args.extend(if literal_user {
                ["-o".to_owned(), format!("User={username}")]
            } else {
                ["-l".to_owned(), username.to_owned()]
            });
        }
        if let Some(port) = port {
            args.extend(["-p".to_owned(), port.to_string()]);
        }
        args.extend(["--".to_owned(), host]);
        return args;
    }
    vec!["--".to_owned(), ssh_config_probe_target(value).into_owned()]
}

pub(super) fn parse_host_port(host_port: &str) -> io::Result<(String, u16)> {
    let (host, port) = parse_host_port_optional(host_port)?;
    Ok((host, port.unwrap_or(22)))
}

pub(super) fn parse_host_port_optional(host_port: &str) -> io::Result<(String, Option<u16>)> {
    let (host, port) = if let Some(rest) = host_port.strip_prefix('[') {
        let (host, suffix) = rest
            .split_once(']')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "无效的 IPv6 SSH 地址"))?;
        let port = suffix
            .strip_prefix(':')
            .map(str::parse)
            .transpose()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "无效的 SSH 端口"))?;
        (host.to_owned(), port)
    } else if let Some((host, port)) = host_port.rsplit_once(':') {
        if host.contains(':') {
            (host_port.to_owned(), None)
        } else {
            let port = port
                .parse()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "无效的 SSH 端口"))?;
            (host.to_owned(), Some(port))
        }
    } else {
        (host_port.to_owned(), None)
    };
    if host.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "SSH 主机为空"));
    }
    Ok((host, port))
}
