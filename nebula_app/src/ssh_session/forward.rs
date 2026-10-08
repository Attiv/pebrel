use std::net::Ipv4Addr;

use tokio::net::TcpListener;
use tokio::task::{JoinHandle, JoinSet};

use super::{NoopSshEventHost, SessionError, SharedSession, SshDestination, authenticated_session};

// Bound per-listener channel tasks and copy buffers; excess clients wait in the TCP backlog.
const MAX_CONNECTIONS: usize = 64;

pub(crate) struct LocalForward {
    local_port: u16,
    remote_port: u16,
    task: JoinHandle<()>,
}

pub(crate) enum PortForward {
    Local(LocalForward),
    Remote(RemoteForward),
}

impl PortForward {
    pub(crate) fn local_port(&self) -> u16 {
        match self {
            Self::Local(forward) => forward.local_port,
            Self::Remote(forward) => forward.local_port,
        }
    }

    pub(crate) fn remote_port(&self) -> u16 {
        match self {
            Self::Local(forward) => forward.remote_port,
            Self::Remote(forward) => forward.remote_port,
        }
    }

    pub(crate) fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

struct RemoteForward {
    local_port: u16,
    remote_port: u16,
    session: SharedSession,
    routes: super::RemoteForwardRoutes,
}

impl Drop for LocalForward {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Drop for RemoteForward {
    fn drop(&mut self) {
        if let Ok(mut routes) = self.routes.lock() {
            routes.remove(&self.remote_port);
        }
        let session = self.session.clone();
        let remote_port = self.remote_port;
        if let Ok(runtime) = super::runtime() {
            runtime.spawn(async move {
                if let Err(error) =
                    session.cancel_tcpip_forward("127.0.0.1", u32::from(remote_port)).await
                {
                    log::debug!("SSH remote port-forward cancellation failed: {error}");
                }
            });
        }
    }
}

pub(crate) async fn open_local_forward(
    raw_destination: &str,
    local_port: u16,
    remote_port: u16,
) -> Result<PortForward, SessionError> {
    let profiles_path = crate::display::nebula_data_dir().join("ssh_profiles.json");
    let raw = raw_destination.to_owned();
    let (destination, profile) = tokio::task::spawn_blocking(move || {
        let destination = SshDestination::resolve(&raw)?;
        let profiles = crate::ssh_profiles::SshProfiles::load(&profiles_path)?;
        Ok::<_, std::io::Error>((destination, profiles.for_destination(&raw)))
    })
    .await
    .map_err(|error| format!("SSH 地址解析任务失败: {error}"))??;

    let session = authenticated_session(&destination, &profile, None::<&NoopSshEventHost>).await?;
    bind_forward(session, local_port, remote_port).await.map(PortForward::Local)
}

async fn bind_forward(
    session: SharedSession,
    local_port: u16,
    remote_port: u16,
) -> Result<LocalForward, SessionError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, local_port)).await?;
    let local_port = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        let mut accept_error_logged = false;
        loop {
            tokio::select! {
                accepted = listener.accept(), if connections.len() < MAX_CONNECTIONS => {
                    match accepted {
                        Ok((mut local, peer)) => {
                            accept_error_logged = false;
                            let session = session.clone();
                            connections.spawn(async move {
                                let channel = super::lifecycle::network(
                                    "port-forward channel",
                                    session.channel_open_direct_tcpip(
                                        Ipv4Addr::LOCALHOST.to_string(),
                                        u32::from(remote_port),
                                        peer.ip().to_string(),
                                        u32::from(peer.port()),
                                    ),
                                )
                                .await?;
                                let mut remote = channel.into_stream();
                                tokio::io::copy_bidirectional(&mut local, &mut remote).await?;
                                Ok::<(), SessionError>(())
                            });
                        },
                        Err(error) => {
                            if !accept_error_logged {
                                log::warn!("SSH port-forward listener accept failed; retrying: {error}");
                                accept_error_logged = true;
                            }
                            // ponytail: retry at most once per second; add backoff if persistent errors warrant it.
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        },
                    }
                },
                Some(result) = connections.join_next(), if !connections.is_empty() => {
                    match result {
                        Ok(Ok(())) => {},
                        Ok(Err(error)) => log::warn!("SSH port-forward connection failed: {error}"),
                        Err(error) => log::warn!("SSH port-forward task failed: {error}"),
                    }
                },
            }
        }
    });

    Ok(LocalForward { local_port, remote_port, task })
}

pub(crate) async fn open_remote_forward(
    raw_destination: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<PortForward, SessionError> {
    let profiles_path = crate::display::nebula_data_dir().join("ssh_profiles.json");
    let raw = raw_destination.to_owned();
    let (destination, profile) = tokio::task::spawn_blocking(move || {
        let destination = SshDestination::resolve(&raw)?;
        let profiles = crate::ssh_profiles::SshProfiles::load(&profiles_path)?;
        Ok::<_, std::io::Error>((destination, profiles.for_destination(&raw)))
    })
    .await
    .map_err(|error| format!("SSH 地址解析任务失败: {error}"))??;

    let acquired =
        super::authenticated_session_at(&destination, &profile, None::<&NoopSshEventHost>, 0)
            .await?;
    {
        let mut routes =
            acquired.remote_forward_routes.lock().map_err(|_| "SSH 远端转发路由表已损坏")?;
        if routes.contains_key(&remote_port) {
            return Err(format!("SSH 远端端口 {remote_port} 已在转发").into());
        }
        routes.insert(remote_port, super::RemoteForwardRoute::Pending);
    }
    if let Err(error) = acquired.session.tcpip_forward("127.0.0.1", u32::from(remote_port)).await {
        if let Ok(mut routes) = acquired.remote_forward_routes.lock() {
            routes.remove(&remote_port);
        }
        return Err(error.into());
    }
    if let Ok(mut routes) = acquired.remote_forward_routes.lock() {
        routes.insert(remote_port, super::RemoteForwardRoute::Active(local_port));
    } else {
        let _ = acquired.session.cancel_tcpip_forward("127.0.0.1", u32::from(remote_port)).await;
        return Err("SSH 远端转发路由表已损坏".into());
    }
    Ok(PortForward::Remote(RemoteForward {
        local_port,
        remote_port,
        session: acquired.session,
        routes: acquired.remote_forward_routes,
    }))
}

pub(super) async fn accept_remote_channel(
    channel: russh::Channel<russh::client::Msg>,
    local_port: Option<u16>,
    reply: russh::client::ChannelOpenHandle,
) {
    let Some(local_port) = local_port else {
        reply.reject(russh::ChannelOpenFailure::AdministrativelyProhibited).await;
        return;
    };
    let local = match tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, local_port)).await {
        Ok(local) => local,
        Err(error) => {
            log::warn!("SSH remote port-forward target connection failed: {error}");
            reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            return;
        },
    };
    reply.accept().await;
    tokio::spawn(async move {
        let mut local = local;
        let mut remote = channel.into_stream();
        if let Err(error) = tokio::io::copy_bidirectional(&mut local, &mut remote).await {
            log::debug!("SSH remote port-forward stream ended: {error}");
        }
    });
}

#[cfg(test)]
mod tests;
