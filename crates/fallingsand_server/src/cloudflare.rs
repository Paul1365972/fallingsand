use anyhow::{Context, bail, ensure};
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    OrderStatus, RetryPolicy,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const CF_API: &str = "https://api.cloudflare.com/client/v4";
const TRACE_URL: &str = "https://one.one.one.one/cdn-cgi/trace";
const RENEW_AFTER: Duration = Duration::from_secs(60 * 24 * 60 * 60);
const DDNS_INTERVAL: Duration = Duration::from_secs(300);
const DNS_TTL: u32 = 60;
const TXT_POLL_INTERVAL: Duration = Duration::from_secs(5);
const TXT_POLL_ATTEMPTS: u32 = 24;

pub struct CloudflareHost {
    client: reqwest::Client,
    token: String,
    zone_id: String,
    domain: String,
}

#[derive(Deserialize)]
struct ApiResponse<T> {
    success: bool,
    result: Option<T>,
    #[serde(default)]
    errors: serde_json::Value,
}

#[derive(Deserialize)]
struct Zone {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct DnsRecord {
    id: String,
    content: String,
}

#[derive(Serialize, Deserialize)]
struct CertMeta {
    domain: String,
    issued_unix: u64,
}

async fn api_call<T: serde::de::DeserializeOwned>(
    token: &str,
    request: reqwest::RequestBuilder,
) -> anyhow::Result<T> {
    let body: ApiResponse<T> = request.bearer_auth(token).send().await?.json().await?;
    ensure!(body.success, "cloudflare api error: {}", body.errors);
    body.result.context("cloudflare api returned no result")
}

impl CloudflareHost {
    pub async fn new(token: String, domain: String) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .local_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
            .build()?;
        let zones: Vec<Zone> =
            api_call(&token, client.get(format!("{CF_API}/zones?per_page=50"))).await?;
        let zone = zones
            .into_iter()
            .filter(|zone| domain == zone.name || domain.ends_with(&format!(".{}", zone.name)))
            .max_by_key(|zone| zone.name.len())
            .with_context(|| format!("no cloudflare zone found for {domain}"))?;
        debug!("cloudflare zone {} ({})", zone.name, zone.id);
        Ok(Self {
            client,
            token,
            zone_id: zone.id,
            domain,
        })
    }

    pub async fn public_ip(&self) -> anyhow::Result<String> {
        let text = self.client.get(TRACE_URL).send().await?.text().await?;
        text.lines()
            .find_map(|line| line.strip_prefix("ip="))
            .map(str::to_string)
            .context("no ip in trace response")
    }

    async fn find_records(&self, kind: &str, name: &str) -> anyhow::Result<Vec<DnsRecord>> {
        let url = format!(
            "{CF_API}/zones/{}/dns_records?type={kind}&name={name}",
            self.zone_id
        );
        api_call(&self.token, self.client.get(url)).await
    }

    async fn upsert_record(&self, kind: &str, name: &str, content: &str) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "type": kind,
            "name": name,
            "content": content,
            "ttl": DNS_TTL,
            "proxied": false,
        });
        match self.find_records(kind, name).await?.first() {
            Some(record) if record.content == content => {}
            Some(record) => {
                let url = format!("{CF_API}/zones/{}/dns_records/{}", self.zone_id, record.id);
                api_call::<serde_json::Value>(&self.token, self.client.put(url).json(&body))
                    .await?;
            }
            None => {
                let url = format!("{CF_API}/zones/{}/dns_records", self.zone_id);
                api_call::<serde_json::Value>(&self.token, self.client.post(url).json(&body))
                    .await?;
            }
        }
        Ok(())
    }

    async fn delete_records(&self, kind: &str, name: &str) -> anyhow::Result<()> {
        for record in self.find_records(kind, name).await? {
            let url = format!("{CF_API}/zones/{}/dns_records/{}", self.zone_id, record.id);
            api_call::<serde_json::Value>(&self.token, self.client.delete(url)).await?;
        }
        Ok(())
    }

    pub async fn ensure_dns(&self) -> anyhow::Result<String> {
        let ip = self.public_ip().await?;
        self.upsert_record("A", &self.domain, &ip).await?;
        Ok(ip)
    }

    pub fn spawn_ddns(self: Arc<Self>, handle: &tokio::runtime::Handle, mut last_ip: String) {
        debug!(
            "watching public ip every {}s for dns updates",
            DDNS_INTERVAL.as_secs()
        );
        handle.spawn(async move {
            loop {
                tokio::time::sleep(DDNS_INTERVAL).await;
                match self.public_ip().await {
                    Ok(ip) if ip != last_ip => {
                        match self.upsert_record("A", &self.domain, &ip).await {
                            Ok(()) => {
                                info!("dns updated: {} -> {ip}", self.domain);
                                last_ip = ip;
                            }
                            Err(err) => warn!("dns update failed: {err:#}"),
                        }
                    }
                    Ok(_) => {}
                    Err(err) => warn!("public ip check failed: {err:#}"),
                }
            }
        });
    }

    pub async fn ensure_certificate(&self, dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
        std::fs::create_dir_all(dir)?;
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        let meta_path = dir.join("meta.json");

        let meta = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|data| serde_json::from_str::<CertMeta>(&data).ok());
        if let Some(meta) = meta {
            let age = unix_now().saturating_sub(meta.issued_unix);
            if meta.domain == self.domain
                && Duration::from_secs(age) < RENEW_AFTER
                && cert_path.exists()
                && key_path.exists()
            {
                info!(
                    "using cached certificate for {} ({} days until renewal)",
                    self.domain,
                    (RENEW_AFTER.as_secs() - age) / 86400
                );
                return Ok((cert_path, key_path));
            }
        }

        info!(
            "requesting let's encrypt certificate for {} (dns-01)",
            self.domain
        );
        let account = self.acme_account(dir).await?;
        let identifiers = [Identifier::Dns(self.domain.clone())];
        let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;
        let challenge_name = format!("_acme-challenge.{}", self.domain);

        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            let mut authz = result?;
            match authz.status {
                AuthorizationStatus::Pending => {}
                AuthorizationStatus::Valid => continue,
                status => bail!("unexpected acme authorization status: {status:?}"),
            }
            let mut challenge = authz
                .challenge(ChallengeType::Dns01)
                .context("no dns-01 challenge offered")?;
            let txt_value = challenge.key_authorization().dns_value();
            self.upsert_record("TXT", &challenge_name, &txt_value)
                .await?;
            debug!("set challenge txt record on {challenge_name}");
            self.wait_for_txt(&challenge_name, &txt_value).await;
            challenge.set_ready().await?;
        }

        let status = order.poll_ready(&RetryPolicy::default()).await;
        let _ = self.delete_records("TXT", &challenge_name).await;
        let status = status?;
        if status != OrderStatus::Ready {
            let mut detail = String::new();
            if let Some(error) = &order.state().error {
                detail.push_str(&format!(": {error}"));
            }
            let mut authorizations = order.authorizations();
            while let Some(Ok(authz)) = authorizations.next().await {
                for challenge in &authz.challenges {
                    if let Some(error) = &challenge.error {
                        detail.push_str(&format!(": {error}"));
                    }
                }
            }
            bail!("acme order failed ({status:?}){detail}");
        }
        let key_pem = order.finalize().await?;
        let cert_pem = order.poll_certificate(&RetryPolicy::default()).await?;

        std::fs::write(&cert_path, cert_pem)?;
        std::fs::write(&key_path, key_pem)?;
        let meta = CertMeta {
            domain: self.domain.clone(),
            issued_unix: unix_now(),
        };
        std::fs::write(&meta_path, serde_json::to_string(&meta)?)?;
        info!("certificate issued, cached in {}", dir.display());
        Ok((cert_path, key_path))
    }

    async fn wait_for_txt(&self, name: &str, value: &str) {
        for attempt in 1..=TXT_POLL_ATTEMPTS {
            tokio::time::sleep(TXT_POLL_INTERVAL).await;
            let records = self.query_txt(name).await.unwrap_or_default();
            if records.iter().any(|data| data.contains(value)) {
                info!(
                    "challenge txt record visible after ~{}s",
                    attempt * TXT_POLL_INTERVAL.as_secs() as u32
                );
                return;
            }
        }
        warn!("challenge txt record still not visible via dns, proceeding anyway");
    }

    async fn query_txt(&self, name: &str) -> anyhow::Result<Vec<String>> {
        let url = format!("https://cloudflare-dns.com/dns-query?name={name}&type=TXT");
        let response: serde_json::Value = self
            .client
            .get(url)
            .header("accept", "application/dns-json")
            .send()
            .await?
            .json()
            .await?;
        Ok(response["Answer"]
            .as_array()
            .map(|answers| {
                answers
                    .iter()
                    .filter_map(|answer| answer["data"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn acme_account(&self, dir: &Path) -> anyhow::Result<Account> {
        let credentials_path = dir.join("account.json");
        if let Ok(data) = std::fs::read_to_string(&credentials_path)
            && let Ok(credentials) = serde_json::from_str(&data)
            && let Ok(account) = Account::builder()?.from_credentials(credentials).await
        {
            debug!("using cached acme account");
            return Ok(account);
        }
        info!("registering new let's encrypt account");
        let (account, credentials) = Account::builder()?
            .create(
                &NewAccount {
                    contact: &[],
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                LetsEncrypt::Production.url().to_owned(),
                None,
            )
            .await?;
        std::fs::write(&credentials_path, serde_json::to_string(&credentials)?)?;
        Ok(account)
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
