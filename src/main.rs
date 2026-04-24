// solinteg mht-10k-25

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::*;

#[derive(Deserialize)]
struct Config {
    host: String,
    port: u16,
    #[serde(default = "default_poll_interval_seconds")]
    poll_interval_seconds: u64,
    grafana: GrafanaConfig,
}

#[derive(Deserialize)]
struct GrafanaConfig {
    push_url: String,
    username: Option<String>,
    api_key: Option<String>,
    tenant_id: Option<String>,
    #[serde(default = "default_grafana_job")]
    job: String,
    device: Option<String>,
}

#[derive(Serialize)]
struct Readings {
    pv_power_kw: f32,
    home_load_kw: f32,
    inverter_temp_c: f32,
    soc_percent: f32,
    battery_current_a: f32,
    battery_power_kw: f32,
}

#[derive(Serialize)]
struct LokiPushRequest {
    streams: Vec<LokiStream>,
}

#[derive(Serialize)]
struct LokiStream {
    stream: LokiLabels,
    values: Vec<[String; 2]>,
}

#[derive(Serialize)]
struct LokiLabels {
    job: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<String>,
}

fn default_poll_interval_seconds() -> u64 {
    5
}

fn default_grafana_job() -> String {
    "solinteg_read".to_string()
}

fn u16_to_i16(v: u16) -> i16 {
    v as i16
}

fn regs_to_i32_be(high: u16, low: u16) -> i32 {
    ((high as u32) << 16 | (low as u32)) as i32
}

fn config_path_from_args(args: &[String], home_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = args.windows(2).find(|w| w[0] == "-c").map(|w| &w[1]) {
        return Ok(PathBuf::from(path));
    }

    let home_dir =
        home_dir.context("failed to determine home directory for default config path")?;
    Ok(home_dir.join(".config/solimon"))
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

async fn read_inverter(socket_addr: SocketAddr) -> Result<Readings> {
    let slave = Slave(255);
    let mut ctx = tcp::connect_slave(socket_addr, slave)
        .await
        .context("failed to connect to inverter")?;

    let pv = ctx
        .read_holding_registers(11028, 2)
        .await
        .context("failed to read PV power registers")??;
    let pv_power_kw = pv[1] as f32 / 1000.0;

    let home_load = ctx
        .read_holding_registers(11016, 2)
        .await
        .context("failed to read home load registers")??;
    let home_load_kw = home_load[1] as f32 / 1000.0;

    let temp = ctx
        .read_holding_registers(11032, 1)
        .await
        .context("failed to read inverter temperature register")??;
    let inverter_temp_c = temp[0] as f32 / 10.0;

    let soc = ctx
        .read_holding_registers(11056, 1)
        .await
        .context("failed to read SOC register")??;
    let soc_percent = soc[0] as f32 / 100.0;

    let batt_i = ctx
        .read_holding_registers(30254, 4)
        .await
        .context("failed to read battery current registers")??;
    let battery_current_a = u16_to_i16(batt_i[1]) as f32 / 10.0;

    let batt_p = ctx
        .read_holding_registers(30258, 2)
        .await
        .context("failed to read battery power registers")??;
    let battery_power_kw = regs_to_i32_be(batt_p[0], batt_p[1]) as f32 / 1000.0;

    Ok(Readings {
        pv_power_kw,
        home_load_kw,
        inverter_temp_c,
        soc_percent,
        battery_current_a,
        battery_power_kw,
    })
}

fn format_readings(readings: &Readings) -> String {
    format!(
        "PV power:        {:.3} kW\nHome load:       {:.3} kW\nInverter temp:   {:.1} °C\nSOC:             {} %\nBattery current: {:.1} A\nBattery power:   {:.3} kW",
        readings.pv_power_kw,
        readings.home_load_kw,
        readings.inverter_temp_c,
        readings.soc_percent,
        readings.battery_current_a,
        readings.battery_power_kw
    )
}

fn current_timestamp_nanos() -> Result<String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX_EPOCH")?
        .as_nanos()
        .to_string())
}

fn build_loki_payload(
    readings: &Readings,
    grafana: &GrafanaConfig,
    timestamp_nanos: String,
) -> Result<LokiPushRequest> {
    let reading_json =
        serde_json::to_string(readings).context("failed to serialize readings for Grafana")?;

    Ok(LokiPushRequest {
        streams: vec![LokiStream {
            stream: LokiLabels {
                job: grafana.job.clone(),
                source: "solinteg-read".to_string(),
                device: grafana.device.clone(),
            },
            values: vec![[timestamp_nanos, reading_json]],
        }],
    })
}

async fn push_to_grafana(
    client: &reqwest::Client,
    grafana: &GrafanaConfig,
    readings: &Readings,
) -> Result<()> {
    let payload = build_loki_payload(readings, grafana, current_timestamp_nanos()?)?;
    let mut request = client.post(&grafana.push_url).json(&payload);

    if let (Some(username), Some(api_key)) = (&grafana.username, &grafana.api_key) {
        request = request.basic_auth(username, Some(api_key));
    }

    if let Some(tenant_id) = &grafana.tenant_id {
        request = request.header("X-Scope-OrgID", tenant_id);
    }

    request
        .send()
        .await
        .context("failed to send readings to Grafana")?
        .error_for_status()
        .context("Grafana rejected readings")?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let json_output = has_flag(&args, "-j");
    let config_path =
        config_path_from_args(&args, std::env::var_os("HOME").as_deref().map(Path::new))?;
    let config_path_display = config_path.display().to_string();

    let config_str = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {config_path_display}"))?;
    let config: Config = toml::from_str(&config_str)
        .with_context(|| format!("failed to parse {config_path_display}"))?;
    let socket_addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .context("failed to parse inverter host/port")?;
    let client = reqwest::Client::new();
    let poll_interval = Duration::from_secs(config.poll_interval_seconds.max(1));
    let mut ticker = tokio::time::interval(poll_interval);

    loop {
        ticker.tick().await;

        match read_inverter(socket_addr).await {
            Ok(readings) => {
                if json_output {
                    println!("{}", serde_json::to_string(&readings)?);
                } else {
                    println!("{}", format_readings(&readings));
                }

                if let Err(err) = push_to_grafana(&client, &config.grafana, &readings).await {
                    eprintln!("grafana push error: {err:#}");
                }
            }
            Err(err) => eprintln!("inverter read error: {err:#}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_path_from_args_uses_explicit_c_value() {
        let args = vec![
            "solinteg-read".to_string(),
            "-c".to_string(),
            "/tmp/custom.toml".to_string(),
        ];

        let path = config_path_from_args(&args, Some(Path::new("/home/test"))).unwrap();

        assert_eq!(path, PathBuf::from("/tmp/custom.toml"));
    }

    #[test]
    fn config_path_from_args_defaults_to_home_config_file() {
        let args = vec!["solinteg-read".to_string()];

        let path = config_path_from_args(&args, Some(Path::new("/home/test"))).unwrap();

        assert_eq!(path, PathBuf::from("/home/test/.config/solimon"));
    }

    #[test]
    fn config_path_from_args_errors_without_home_or_c_flag() {
        let args = vec!["solinteg-read".to_string()];

        let err = config_path_from_args(&args, None).unwrap_err();

        assert!(err
            .to_string()
            .contains("failed to determine home directory"));
    }

    #[test]
    fn has_flag_detects_present_flag() {
        let args = vec!["solinteg-read".to_string(), "-j".to_string()];

        assert!(has_flag(&args, "-j"));
    }

    #[test]
    fn has_flag_rejects_missing_flag() {
        let args = vec!["solinteg-read".to_string(), "-c".to_string()];

        assert!(!has_flag(&args, "-j"));
    }

    #[test]
    fn readings_serialize_to_json_keys() {
        let readings = Readings {
            pv_power_kw: 1.234,
            home_load_kw: 2.345,
            inverter_temp_c: 30.5,
            soc_percent: 88.0,
            battery_current_a: -4.2,
            battery_power_kw: -1.111,
        };

        let json = serde_json::to_string(&readings).unwrap();

        assert_eq!(
            json,
            "{\"pv_power_kw\":1.234,\"home_load_kw\":2.345,\"inverter_temp_c\":30.5,\"soc_percent\":88.0,\"battery_current_a\":-4.2,\"battery_power_kw\":-1.111}"
        );
    }

    #[test]
    fn config_defaults_poll_interval_and_job() {
        let config: Config = toml::from_str(
            r#"
host = "192.168.1.142"
port = 502

[grafana]
push_url = "https://logs-prod.example.com/loki/api/v1/push"
"#,
        )
        .unwrap();

        assert_eq!(config.poll_interval_seconds, 5);
        assert_eq!(config.grafana.job, "solinteg_read");
    }

    #[test]
    fn build_loki_payload_includes_all_readings() {
        let grafana = GrafanaConfig {
            push_url: "https://logs-prod.example.com/loki/api/v1/push".to_string(),
            username: Some("user".to_string()),
            api_key: Some("secret".to_string()),
            tenant_id: None,
            job: "solinteg_read".to_string(),
            device: Some("garage-inverter".to_string()),
        };
        let readings = Readings {
            pv_power_kw: 1.234,
            home_load_kw: 2.345,
            inverter_temp_c: 30.5,
            soc_percent: 88.0,
            battery_current_a: -4.2,
            battery_power_kw: -1.111,
        };

        let payload = build_loki_payload(&readings, &grafana, "123".to_string()).unwrap();
        let json = serde_json::to_value(payload).unwrap();

        assert_eq!(json["streams"][0]["stream"]["job"], "solinteg_read");
        assert_eq!(json["streams"][0]["stream"]["device"], "garage-inverter");
        assert_eq!(json["streams"][0]["values"][0][0], "123");
        assert_eq!(
            json["streams"][0]["values"][0][1],
            "{\"pv_power_kw\":1.234,\"home_load_kw\":2.345,\"inverter_temp_c\":30.5,\"soc_percent\":88.0,\"battery_current_a\":-4.2,\"battery_power_kw\":-1.111}"
        );
    }
}
