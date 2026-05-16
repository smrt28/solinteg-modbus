use super::*;
use std::path::{Path, PathBuf};

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
fn has_flag_detects_help_flag() {
    let args = vec!["solinteg-read".to_string(), "-h".to_string()];

    assert!(has_flag(&args, "-h"));
}

#[test]
fn short_help_describes_supported_flags() {
    let help = short_help();

    assert!(help.contains("Usage: solinteg-read"));
    assert!(help.contains("-c <config>"));
    assert!(help.contains("-j"));
    assert!(help.contains("-1"));
    assert!(help.contains("-h"));
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
