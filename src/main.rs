// solinteg mht-10k-25

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use log::info;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::*;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
struct Config {
    host: String,
    port: u16,
    #[serde(default = "default_poll_interval_seconds")]
    poll_interval_seconds: u64,
    #[serde(default = "default_read_timeout_seconds")]
    read_timeout_seconds: u64,
    #[serde(alias = "grafana")]
    influxdb: InfluxDbConfig,
}

#[derive(Deserialize)]
struct InfluxDbConfig {
    write_url: String,
    token: Option<String>,
    username: Option<String>,
    password: Option<String>,
    database: Option<String>,
    org: Option<String>,
    bucket: Option<String>,
    #[serde(default = "default_measurement")]
    measurement: String,
    device: Option<String>,
}

#[derive(Serialize)]
struct Readings {
    pv_power_kw: f32,
    grid_power_kw: f32,
    home_load_kw: f32,
    inverter_temp_c: f32,
    soc_percent: f32,
    battery_current_a: f32,
    battery_power_kw: f32,
}

#[derive(Debug, Parser)]
#[command(about = "Read Solinteg inverter data over Modbus TCP")]
struct Cli {
    #[arg(short = 'c', value_name = "config", help = "Read config from path")]
    config: Option<PathBuf>,
    #[arg(short = 'j', long = "json", help = "Print readings as JSON")]
    json: bool,
    #[arg(short = '1', help = "Read once and exit")]
    one: bool,
    #[arg(long = "dump-config", help = "Print configured values and exit")]
    dump_config: bool,
}

#[derive(Serialize)]
struct ConfigDump<'a> {
    host: &'a str,
    port: u16,
    poll_interval_seconds: u64,
    read_timeout_seconds: u64,
    influxdb: InfluxDbConfigDump<'a>,
}

#[derive(Serialize)]
struct InfluxDbConfigDump<'a> {
    write_url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    org: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bucket: Option<&'a str>,
    measurement: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<&'a str>,
}

fn default_poll_interval_seconds() -> u64 {
    5
}

fn default_read_timeout_seconds() -> u64 {
    5
}

fn default_measurement() -> String {
    "solinteg_readings".to_string()
}

fn u16_to_i16(v: u16) -> i16 {
    v as i16
}

fn regs_to_i32_be(high: u16, low: u16) -> i32 {
    ((high as u32) << 16 | (low as u32)) as i32
}

fn config_path_from_cli(cli: &Cli, home_dir: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = &cli.config {
        return Ok(path.clone());
    }

    let home_dir =
        home_dir.context("failed to determine home directory for default config path")?;
    Ok(home_dir.join(".config/solimon"))
}

fn configured_secret_marker(value: &Option<String>) -> Option<&'static str> {
    value.as_ref().map(|_| "[redacted]")
}

fn dump_config(config: &Config) -> Result<String> {
    let dump = ConfigDump {
        host: &config.host,
        port: config.port,
        poll_interval_seconds: config.poll_interval_seconds,
        read_timeout_seconds: config.read_timeout_seconds,
        influxdb: InfluxDbConfigDump {
            write_url: &config.influxdb.write_url,
            token: configured_secret_marker(&config.influxdb.token),
            username: config.influxdb.username.as_deref(),
            password: configured_secret_marker(&config.influxdb.password),
            database: config.influxdb.database.as_deref(),
            org: config.influxdb.org.as_deref(),
            bucket: config.influxdb.bucket.as_deref(),
            measurement: &config.influxdb.measurement,
            device: config.influxdb.device.as_deref(),
        },
    };

    toml::to_string_pretty(&dump).context("failed to serialize config dump")
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

    let grid_load = ctx // 11060
        .read_holding_registers(11058, 2)
        .await
        .context("failed to read grid load registers")??;
    let grid_power_kw = grid_load[1] as f32 / 1000.0;

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
        grid_power_kw,
        home_load_kw,
        inverter_temp_c,
        soc_percent,
        battery_current_a,
        battery_power_kw,
    })
}

async fn read_inverter_with_timeout(
    socket_addr: SocketAddr,
    read_timeout: Duration,
) -> Result<Readings> {
    tokio::time::timeout(read_timeout, read_inverter(socket_addr))
        .await
        .with_context(|| {
            format!(
                "inverter did not respond within {} seconds",
                read_timeout.as_secs()
            )
        })?
}

fn format_readings(readings: &Readings) -> String {
    format!(
        "PV power:        {:.3} kW\nGrid power:      {:.3} kW\nHome load:       {:.3} kW\nInverter temp:   {:.1} °C\nSOC:             {} %\nBattery current: {:.1} A\nBattery power:   {:.3} kW",
        readings.pv_power_kw,
        readings.grid_power_kw,
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

fn format_human_timestamp(timestamp: SystemTime) -> String {
    let timestamp: DateTime<Utc> = timestamp.into();
    timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn current_human_timestamp() -> String {
    format_human_timestamp(SystemTime::now())
}

fn prefix_output_with_timestamp(timestamp: &str, output: &str) -> String {
    output
        .lines()
        .map(|line| format!("{timestamp} {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn print_timestamped_output(output: &str) -> Result<()> {
    println!(
        "{}",
        prefix_output_with_timestamp(&current_human_timestamp(), output)
    );
    Ok(())
}

fn print_timestamped_error(output: &str) -> Result<()> {
    eprintln!(
        "{}",
        prefix_output_with_timestamp(&current_human_timestamp(), output)
    );
    Ok(())
}

fn escape_tag(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace(',', "\\,")
        .replace(' ', "\\ ")
        .replace('=', "\\=")
}

fn build_influx_line_protocol(
    readings: &Readings,
    influxdb: &InfluxDbConfig,
    timestamp_nanos: &str,
) -> String {
    let mut line = format!("{},source=solinteg-read", escape_tag(&influxdb.measurement));

    if let Some(device) = &influxdb.device {
        line.push_str(",device=");
        line.push_str(&escape_tag(device));
    }

    line.push_str(&format!(
        " pv_power_kw={},grid_power_kw={},home_load_kw={},inverter_temp_c={},soc_percent={},battery_current_a={},battery_power_kw={} {}",
        readings.pv_power_kw,
        readings.grid_power_kw,
        readings.home_load_kw,
        readings.inverter_temp_c,
        readings.soc_percent,
        readings.battery_current_a,
        readings.battery_power_kw,
        timestamp_nanos
    ));

    line
}

fn influx_write_url(influxdb: &InfluxDbConfig) -> Result<reqwest::Url> {
    let mut url =
        reqwest::Url::parse(&influxdb.write_url).context("failed to parse InfluxDB write_url")?;
    {
        let mut pairs = url.query_pairs_mut();
        if let Some(database) = &influxdb.database {
            pairs.append_pair("db", database);
        }
        if let Some(org) = &influxdb.org {
            pairs.append_pair("org", org);
        }
        if let Some(bucket) = &influxdb.bucket {
            pairs.append_pair("bucket", bucket);
        }
        pairs.append_pair("precision", "ns");
    }
    Ok(url)
}

async fn push_to_influxdb(
    client: &reqwest::Client,
    influxdb: &InfluxDbConfig,
    readings: &Readings,
) -> Result<()> {
    let timestamp_nanos = current_timestamp_nanos()?;
    let payload = build_influx_line_protocol(readings, influxdb, &timestamp_nanos);
    let write_url = influx_write_url(influxdb)?;
    let mut request = client
        .post(write_url)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(payload);

    if let Some(token) = &influxdb.token {
        request = request.header("Authorization", format!("Token {token}"));
    }

    if let (Some(username), Some(password)) = (&influxdb.username, &influxdb.password) {
        request = request.basic_auth(username, Some(password));
    }

    request
        .send()
        .await
        .context("failed to send readings to InfluxDB")?
        .error_for_status()
        .context("InfluxDB rejected readings")?;

    Ok(())
}

fn check_readings_consistency(readings: &Readings) -> bool {
    if readings.pv_power_kw > 60.0 ||
        readings.grid_power_kw > 60.0 ||
        readings.home_load_kw > 60.0 ||
        readings.soc_percent > 100.0 ||
        readings.soc_percent < 0.0 {
        return false;
    }
    true
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let json_output = cli.json;
    let one = cli.one;
    let config_path =
        config_path_from_cli(&cli, std::env::var_os("HOME").as_deref().map(Path::new))?;
    let config_path_display = config_path.display().to_string();

    let config_str = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {config_path_display}"))?;
    let config: Config = toml::from_str(&config_str)
        .with_context(|| format!("failed to parse {config_path_display}"))?;
    if cli.dump_config {
        print!("{}", dump_config(&config)?);
        return Ok(());
    }

    let socket_addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .context("failed to parse inverter host/port")?;
    let client = reqwest::Client::new();
    let poll_interval = Duration::from_secs(config.poll_interval_seconds.max(1));
    let read_timeout = Duration::from_secs(config.read_timeout_seconds.max(1));
    let mut ticker = tokio::time::interval(poll_interval);

    if one {
        match read_inverter_with_timeout(socket_addr, read_timeout).await {
            Ok(readings) => {
                if !check_readings_consistency(&readings) {
                    info!("reading failed");
                    return Ok(());
                }
                if json_output {
                    println!("{}", serde_json::to_string(&readings)?);
                } else {
                    println!("{}", format_readings(&readings));
                }
            }
            Err(err) => eprintln!("inverter read error: {err:#}"),
        }
        return Ok(());
    }

    loop {
        ticker.tick().await;
        match read_inverter_with_timeout(socket_addr, read_timeout).await {
            Ok(readings) => {
                if !check_readings_consistency(&readings) {
                    info!("reading failed");
                    continue;
                }
                if json_output {
                    print_timestamped_output(&serde_json::to_string(&readings)?)?;
                } else {
                    print_timestamped_output(&format_readings(&readings))?;
                }

                if !one {
                    if let Err(err) = push_to_influxdb(&client, &config.influxdb, &readings).await {
                        print_timestamped_error(&format!("influxdb push error: {err:#}"))?;
                    }
                }
            }
            Err(err) => print_timestamped_error(&format!("inverter read error: {err:#}"))?,
        }

        if one {
            break;
        }
    }
    Ok(())
}
