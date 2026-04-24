// solinteg mht-10k-25

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::*;

#[derive(Deserialize)]
struct Config {
    host: String,
    port: u16,
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

    let home_dir = home_dir.context("failed to determine home directory for default config path")?;
    Ok(home_dir.join(".config/solimon"))
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let json_output = has_flag(&args, "-j");
    let config_path = config_path_from_args(
        &args,
        std::env::var_os("HOME").as_deref().map(Path::new),
    )?;
    let config_path_display = config_path.display().to_string();

    let config_str = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {config_path_display}"))?;
    let config: Config = toml::from_str(&config_str)
        .with_context(|| format!("failed to parse {config_path_display}"))?;

    let socket_addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    let slave = Slave(255);
    let mut ctx = tcp::connect_slave(socket_addr, slave).await?;

    let pv = ctx.read_holding_registers(11028, 2).await??;
    let pv_power_kw = pv[1] as f32 / 1000.0;

    let home_load = ctx.read_holding_registers(11016, 2).await??;
    let home_load_kw = home_load[1] as f32 / 1000.0;

    let temp = ctx.read_holding_registers(11032, 1).await??;
    let inverter_temp_c = temp[0] as f32 / 10.0;

    let soc = ctx.read_holding_registers(11056, 1).await??;
    let soc_percent = soc[0] as f32 / 100.0;

    let batt_i = ctx.read_holding_registers(30254, 4).await??;
    let battery_current_a = u16_to_i16(batt_i[1]) as f32 / 10.0;

    let batt_p = ctx.read_holding_registers(30258, 2).await??;
    let battery_power_kw = regs_to_i32_be(batt_p[0], batt_p[1]) as f32 / 1000.0;

    let readings = Readings {
        pv_power_kw,
        home_load_kw,
        inverter_temp_c,
        soc_percent,
        battery_current_a,
        battery_power_kw,
    };

    if json_output {
        println!("{}", serde_json::to_string(&readings)?);
    } else {
        println!("PV power:        {:.3} kW", readings.pv_power_kw);
        println!("Home load:       {:.3} kW", readings.home_load_kw);
        println!("Inverter temp:   {:.1} °C", readings.inverter_temp_c);
        println!("SOC:             {} %", readings.soc_percent);
        println!("Battery current: {:.1} A", readings.battery_current_a);
        println!("Battery power:   {:.3} kW", readings.battery_power_kw);
    }

    Ok(())
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
}
