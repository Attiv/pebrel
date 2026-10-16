//! Conservative fallback for simple ssh_config files when OpenSSH cannot expand them.
//! OpenSSH remains authoritative for Include, Match, tokens and other advanced options.

use super::probe::parse_host_port_optional;
use super::{RemoteForwardSpec, SshDestination, default_identity_files, expand_home};
use std::io;

pub(super) fn resolve_from_ssh_config(original: &str) -> io::Result<Option<SshDestination>> {
    match crate::ssh::read_ssh_config() {
        Ok(text) => resolve_from_ssh_config_text(original, &text),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn invalid(detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Cannot safely expand SSH config without OpenSSH: {detail}"),
    )
}

pub(super) fn resolve_from_ssh_config_text(
    original: &str,
    text: &str,
) -> io::Result<Option<SshDestination>> {
    let address = original.strip_prefix("ssh://").unwrap_or(original);
    let (explicit_user, host_port) =
        address.rsplit_once('@').map_or((None, address), |(user, target)| (Some(user), target));
    let (alias, explicit_port) = parse_host_port_optional(host_port)?;
    let (mut user, mut hostname, mut port, mut proxy_jump) = (None, None, None, None);
    let mut identities = Vec::new();
    let mut remote_forwards = Vec::new();
    let mut exit_on_forward_failure = None;
    let mut identities_specified = false;
    let mut active = true;
    let mut matched = false;
    for line in text.lines() {
        let tokens = crate::ssh::ssh_config_tokens_checked(line)?;
        let Some(keyword) = tokens.first() else { continue };
        let keyword = keyword.to_ascii_lowercase();
        let values = &tokens[1..];
        if matches!(keyword.as_str(), "include" | "match") {
            return Err(invalid(format!("{keyword} requires the system SSH client")));
        }
        if keyword == "host" {
            if values.is_empty() {
                return Err(invalid("Host has no patterns"));
            }
            active = block_matches_alias(values, &alias);
            matched |= active;
            continue;
        }
        if !active {
            continue;
        }
        if keyword == "remoteforward" {
            let forward = parse_remote_forward(values)
                .ok_or_else(|| invalid("unsupported RemoteForward; use loopback TCP endpoints"))?;
            remote_forwards.push(forward);
            matched = true;
            continue;
        }
        let [value] = values else { return Err(invalid(format!("invalid {keyword} value"))) };
        if value.contains('%') || value.contains("${") {
            return Err(invalid(format!("{keyword} expansion requires the system SSH client")));
        }
        matched = true;
        match keyword.as_str() {
            "user" => {
                user.get_or_insert_with(|| value.clone());
            },
            "hostname" => {
                hostname.get_or_insert_with(|| value.clone());
            },
            "port" => {
                let parsed = value
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or_else(|| invalid("invalid Port"))?;
                port.get_or_insert(parsed);
            },
            "identityfile" => {
                identities_specified = true;
                if !value.eq_ignore_ascii_case("none") {
                    let path = expand_home(value);
                    if !identities.contains(&path) {
                        identities.push(path);
                    }
                }
            },
            // Keep `none` as a set value so later wildcard blocks cannot override it.
            "proxyjump" => {
                proxy_jump.get_or_insert_with(|| value.clone());
            },
            "exitonforwardfailure" => {
                let parsed = parse_yes_no(value)
                    .ok_or_else(|| invalid("invalid ExitOnForwardFailure; expected yes or no"))?;
                exit_on_forward_failure.get_or_insert(parsed);
            },
            "proxycommand" if value.eq_ignore_ascii_case("none") => {},
            _ => return Err(invalid(format!("{keyword} requires the system SSH client"))),
        }
    }
    if !matched {
        return Ok(None);
    }
    let user = explicit_user
        .map(str::to_owned)
        .or(user)
        .or_else(|| std::env::var("USERNAME").ok())
        .or_else(|| std::env::var("USER").ok())
        .ok_or_else(|| invalid("no SSH username available"))?;
    let host = hostname.unwrap_or(alias);
    if user.is_empty()
        || host.is_empty()
        || host.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(invalid("invalid User or HostName"));
    }
    Ok(Some(SshDestination {
        original: original.to_owned(),
        user,
        host,
        port: explicit_port.or(port).unwrap_or(22),
        identity_files: if identities_specified { identities } else { default_identity_files() },
        proxy_jump: proxy_jump.filter(|jump| !jump.eq_ignore_ascii_case("none")),
        remote_forwards,
        exit_on_forward_failure: exit_on_forward_failure.unwrap_or(false),
    }))
}

fn block_matches_alias(patterns: &[String], alias: &str) -> bool {
    let mut positive = false;
    for pattern in patterns {
        if let Some(negative) = pattern.strip_prefix('!') {
            if glob_matches(negative, alias) {
                return false;
            }
        } else if glob_matches(pattern, alias) {
            positive = true;
        }
    }
    positive
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase().chars().collect::<Vec<_>>();
    let value = value.to_ascii_lowercase().chars().collect::<Vec<_>>();
    let (mut pattern_index, mut value_index) = (0, 0);
    let mut star = None;
    let mut star_value_index = 0;
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == value[value_index])
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some(pattern_index);
            star_value_index = value_index;
            pattern_index += 1;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

pub(crate) fn parse_resolved_config(original: &str, text: &str) -> Option<SshDestination> {
    let mut user = None;
    let mut host = None;
    let mut port = None;
    let mut identity_files = Vec::new();
    let mut proxy_jump = None;
    let mut remote_forwards = Vec::new();
    let mut exit_on_forward_failure = None;
    for line in text.lines() {
        let tokens = crate::ssh::ssh_config_tokens(line);
        let Some(key) = tokens.first() else { continue };
        let value = tokens[1..].join(" ");
        if value.is_empty() {
            continue;
        }
        match key.to_ascii_lowercase().as_str() {
            "user" if user.is_none() => user = Some(value.to_owned()),
            "hostname" if host.is_none() => host = Some(value.to_owned()),
            "port" if port.is_none() => port = value.parse().ok(),
            "identityfile" => {
                if !value.eq_ignore_ascii_case("none") {
                    identity_files.push(expand_home(&value));
                }
            },
            "proxyjump" if !value.eq_ignore_ascii_case("none") => {
                proxy_jump = Some(value);
            },
            "exitonforwardfailure" => {
                exit_on_forward_failure.get_or_insert(parse_yes_no(&value)?);
            },
            "remoteforward" => {
                remote_forwards.push(parse_remote_forward(&tokens[1..])?);
            },
            _ => {},
        }
    }
    Some(SshDestination {
        original: original.to_owned(),
        user: user?,
        host: host?,
        port: port.unwrap_or(22),
        identity_files,
        proxy_jump,
        remote_forwards,
        exit_on_forward_failure: exit_on_forward_failure.unwrap_or(false),
    })
}

pub(super) fn parse_remote_forward(values: &[String]) -> Option<RemoteForwardSpec> {
    if values.len() != 2 {
        return None;
    }
    // OpenSSH's `ssh -G` brackets TCP hosts, including IPv4 addresses and names.
    let remote_port = if let Some((bind_host, port)) = values[0].rsplit_once(':') {
        if !matches!(bind_host, "localhost" | "127.0.0.1" | "[localhost]" | "[127.0.0.1]") {
            return None;
        }
        port
    } else {
        values[0].as_str()
    }
    .parse::<u16>()
    .ok()
    .filter(|port| *port != 0)?;
    let (local_host, local_port) = values[1].rsplit_once(':')?;
    if !matches!(local_host, "localhost" | "127.0.0.1" | "[localhost]" | "[127.0.0.1]") {
        return None;
    }
    let local_port = local_port.parse::<u16>().ok().filter(|port| *port != 0)?;
    Some(RemoteForwardSpec { remote_port, local_port })
}

fn parse_yes_no(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("yes") {
        Some(true)
    } else if value.eq_ignore_ascii_case("no") {
        Some(false)
    } else {
        None
    }
}
