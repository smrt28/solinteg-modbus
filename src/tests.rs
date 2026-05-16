use super::*;
use clap::CommandFactory;
use std::path::{Path, PathBuf};

#[test]
fn cli_parses_flags() {
    let cli = Cli::try_parse_from([
        "solinteg-read",
        "-c",
        "/tmp/custom.toml",
        "-j",
        "-1",
        "--dump-config",
    ])
    .unwrap();

    assert_eq!(cli.config, Some(PathBuf::from("/tmp/custom.toml")));
    assert!(cli.json);
    assert!(cli.one);
    assert!(cli.dump_config);
}

#[test]
fn config_path_from_cli_uses_explicit_c_value() {
    let cli = Cli::try_parse_from(["solinteg-read", "-c", "/tmp/custom.toml"]).unwrap();

    let path = config_path_from_cli(&cli, Some(Path::new("/home/test"))).unwrap();

    assert_eq!(path, PathBuf::from("/tmp/custom.toml"));
}

#[test]
fn config_path_from_cli_defaults_to_home_config_file() {
    let cli = Cli::try_parse_from(["solinteg-read"]).unwrap();

    let path = config_path_from_cli(&cli, Some(Path::new("/home/test"))).unwrap();

    assert_eq!(path, PathBuf::from("/home/test/.config/solimon"));
}

#[test]
fn config_path_from_cli_errors_without_home_or_c_flag() {
    let cli = Cli::try_parse_from(["solinteg-read"]).unwrap();

    let err = config_path_from_cli(&cli, None).unwrap_err();

    assert!(err
        .to_string()
        .contains("failed to determine home directory"));
}

#[test]
fn cli_rejects_unknown_flags() {
    let err = Cli::try_parse_from(["solinteg-read", "--bogus"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
}

#[test]
fn cli_help_describes_supported_flags() {
    let mut command = Cli::command();
    let mut help = Vec::new();

    command.write_help(&mut help).unwrap();
    let help = String::from_utf8(help).unwrap();

    assert!(help.contains("Usage:"));
    assert!(help.contains("-c <config>"));
    assert!(help.contains("-j"));
    assert!(help.contains("-1"));
    assert!(help.contains("-h"));
    assert!(help.contains("--dump-config"));
}

#[test]
fn readings_serialize_to_json_keys() {
    let readings = Readings {
        pv_power_kw: 1.234,
        grid_power_kw: 3.456,
        home_load_kw: 2.345,
        inverter_temp_c: 30.5,
        soc_percent: 88.0,
        battery_current_a: -4.2,
        battery_power_kw: -1.111,
    };

    let json = serde_json::to_string(&readings).unwrap();

    assert_eq!(
        json,
        "{\"pv_power_kw\":1.234,\"grid_power_kw\":3.456,\"home_load_kw\":2.345,\"inverter_temp_c\":30.5,\"soc_percent\":88.0,\"battery_current_a\":-4.2,\"battery_power_kw\":-1.111}"
    );
}

#[test]
fn prefix_output_with_timestamp_prefixes_each_line() {
    let output = prefix_output_with_timestamp("2026-05-16 14:03:22 UTC", "first\nsecond");

    assert_eq!(
        output,
        "2026-05-16 14:03:22 UTC first\n2026-05-16 14:03:22 UTC second"
    );
}

#[test]
fn format_human_timestamp_uses_utc_calendar_time() {
    let output = format_human_timestamp(SystemTime::UNIX_EPOCH);

    assert_eq!(output, "1970-01-01 00:00:00 UTC");
}

#[test]
fn config_defaults_poll_interval_and_measurement() {
    let config: Config = toml::from_str(
        r#"
host = "192.168.1.142"
port = 502

[influxdb]
write_url = "https://influx.example.com/api/v2/write"
"#,
    )
    .unwrap();

    assert_eq!(config.poll_interval_seconds, 5);
    assert_eq!(config.influxdb.measurement, "solinteg_readings");
}

#[test]
fn dump_config_includes_defaults_and_redacts_secrets() {
    let config: Config = toml::from_str(
        r#"
host = "192.168.1.142"
port = 502

[influxdb]
write_url = "https://influx.example.com/api/v2/write"
token = "secret-token"
password = "secret-password"
username = "reader"
bucket = "solar"
"#,
    )
    .unwrap();

    let dump = dump_config(&config).unwrap();

    assert!(dump.contains("host = \"192.168.1.142\""));
    assert!(dump.contains("poll_interval_seconds = 5"));
    assert!(dump.contains("read_timeout_seconds = 5"));
    assert!(dump.contains("measurement = \"solinteg_readings\""));
    assert!(dump.contains("token = \"[redacted]\""));
    assert!(dump.contains("password = \"[redacted]\""));
    assert!(dump.contains("username = \"reader\""));
    assert!(!dump.contains("secret-token"));
    assert!(!dump.contains("secret-password"));
}

#[test]
fn build_influx_line_protocol_includes_all_readings() {
    let influxdb = InfluxDbConfig {
        write_url: "https://influx.example.com/api/v2/write".to_string(),
        token: Some("secret".to_string()),
        username: None,
        password: None,
        database: None,
        org: Some("home".to_string()),
        bucket: Some("solar".to_string()),
        device: Some("garage-inverter".to_string()),
        measurement: "solinteg_readings".to_string(),
    };
    let readings = Readings {
        pv_power_kw: 1.234,
        grid_power_kw: 3.456,
        home_load_kw: 2.345,
        inverter_temp_c: 30.5,
        soc_percent: 88.0,
        battery_current_a: -4.2,
        battery_power_kw: -1.111,
    };

    let payload = build_influx_line_protocol(&readings, &influxdb, "123");

    assert_eq!(
        payload,
        "solinteg_readings,source=solinteg-read,device=garage-inverter pv_power_kw=1.234,grid_power_kw=3.456,home_load_kw=2.345,inverter_temp_c=30.5,soc_percent=88,battery_current_a=-4.2,battery_power_kw=-1.111 123"
    );
}

#[test]
fn influx_write_url_includes_query_parameters() {
    let influxdb = InfluxDbConfig {
        write_url: "https://influx.example.com/api/v2/write".to_string(),
        token: None,
        username: None,
        password: None,
        database: Some("solinteg".to_string()),
        org: Some("home".to_string()),
        bucket: Some("solar".to_string()),
        measurement: "solinteg_readings".to_string(),
        device: None,
    };

    let url = influx_write_url(&influxdb).unwrap();

    assert_eq!(
        url.as_str(),
        "https://influx.example.com/api/v2/write?db=solinteg&org=home&bucket=solar&precision=ns"
    );
}
