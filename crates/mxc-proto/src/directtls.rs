//! XEP-0368 direct-TLS connector with a STARTTLS fallback.
//!
//! Some networks/firewalls block the STARTTLS port (5222) while leaving the direct-TLS port
//! (5223) open. This [`ServerConnector`] tries **direct TLS on 5223 first** (SRV
//! `_xmpps-client._tcp`, falling back to `<domain>:5223`) and only falls back to the STARTTLS
//! path (SRV `_xmpp-client._tcp`, port 5222) if the direct attempt fails.
//!
//! As of tokio-xmpp 6 the library ships both a [`DirectTlsServerConnector`] and a
//! [`StartTlsServerConnector`] (each driven by a [`DnsConfig`] that does the SRV resolution and
//! cert validation against the JID domain), so this connector is now just a thin "prefer direct,
//! fall back to STARTTLS" composition of the two. Both produce the same `Stream` type, so the
//! fallback can be returned directly.

use tokio_xmpp::connect::{
    DirectTlsServerConnector, DnsConfig, ServerConnector, StartTlsServerConnector,
};
use tokio_xmpp::jid::Jid;
use tokio_xmpp::xmlstream::{PendingFeaturesRecv, Timeouts};
use tokio_xmpp::Error;

use sasl::common::ChannelBinding;

/// Connector preferring direct TLS (5223), falling back to STARTTLS (5222 + SRV).
#[derive(Clone, Debug)]
pub struct PreferDirectTls;

impl ServerConnector for PreferDirectTls {
    // Both library connectors yield the same concrete stream type, so the fallback result can be
    // returned without wrapping.
    type Stream = <DirectTlsServerConnector as ServerConnector>::Stream;

    async fn connect(
        &self,
        jid: &Jid,
        ns: &'static str,
        timeouts: Timeouts,
    ) -> Result<(PendingFeaturesRecv<Self::Stream>, ChannelBinding), Error> {
        let domain = jid.domain();

        // Direct TLS on 5223 (XEP-0368). DnsConfig::srv_xmpps does the `_xmpps-client._tcp`
        // SRV lookup and the `<domain>:5223` fallback; the connector validates the certificate
        // against the JID domain (never the SRV target) per RFC 6120 §3.2.1 / XEP-0368.
        let direct = DirectTlsServerConnector::from(DnsConfig::srv_xmpps(domain.as_str()));
        match direct.connect(jid, ns, timeouts).await {
            Ok(ok) => {
                tracing::info!("connected via direct TLS (port 5223)");
                Ok(ok)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "direct TLS on 5223 failed; falling back to STARTTLS on 5222"
                );
                let starttls =
                    StartTlsServerConnector::from(DnsConfig::srv_default_client(domain.as_str()));
                starttls.connect(jid, ns, timeouts).await
            }
        }
    }
}
