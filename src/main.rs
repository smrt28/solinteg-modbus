// solinteg mht-10k-25

use anyhow::{Context, Result};
use serde::Deserialize;
use std::net::SocketAddr;
use tokio_modbus::client::tcp;
use tokio_modbus::prelude::*;

#[derive(Deserialize)]
struct Config {
    host: String,
    port: u16,
}

fn u16_to_i16(v: u16) -> i16 {
    v as i16
}

fn regs_to_i32_be(high: u16, low: u16) -> i32 {
    ((high as u32) << 16 | (low as u32)) as i32
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let config_path = args
        .windows(2)
        .find(|w| w[0] == "-c")
        .map(|w| &w[1])
        .context("usage: solinteg-read -c config.toml")?;

    let config_str = std::fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {config_path}"))?;
    let config: Config = toml::from_str(&config_str)
        .with_context(|| format!("failed to parse {config_path}"))?;

    let socket_addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    let slave = Slave(255);
    let mut ctx = tcp::connect_slave(socket_addr, slave).await?;

    let pv = ctx.read_holding_registers(11028, 2).await??;
    let pv_power_kw = pv[1] as f32 / 1000.0;

    let temp = ctx.read_holding_registers(11032, 1).await??;
    let inverter_temp_c = temp[0] as f32 / 10.0;

    let soc = ctx.read_holding_registers(11056, 1).await??;
    let soc_percent = soc[0] as f32 / 100.0;

    let batt_i = ctx.read_holding_registers(30254, 4).await??;
    let battery_current_a = u16_to_i16(batt_i[1]) as f32 / 10.0;

    let batt_p = ctx.read_holding_registers(30258, 2).await??;
    let battery_power_kw = regs_to_i32_be(batt_p[0], batt_p[1]) as f32 / 1000.0;

    println!("PV power:        {:.3} kW", pv_power_kw);
    println!("Inverter temp:   {:.1} °C", inverter_temp_c);
    println!("SOC:             {:} %", soc_percent);
    println!("Battery current: {:.1} A", battery_current_a);
    println!("Battery power:   {:.3} kW", battery_power_kw);

    Ok(())
}
