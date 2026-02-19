use flexi_logger::*;
use hbb_common::{bail, config::RENDEZVOUS_PORT, ResultType};
use hbbs::{common::*, *};
use std::net::SocketAddr;
use std::str::FromStr;

const RMEM: usize = 0;

fn main() -> ResultType<()> {
    let _logger = Logger::try_with_env_or_str("info")?
        .log_to_stdout()
        .format(opt_format)
        .write_mode(WriteMode::Async)
        .start()?;

    // 新增 -b/--bind 参数说明，默认双栈监听
    let args = format!(
        "-c --config=[FILE] +takes_value 'Sets a custom config file'
        -p, --port=[NUMBER(default={RENDEZVOUS_PORT})] 'Sets the listening port (deprecated, use -b instead)'
        -b, --bind=[ADDRS] +takes_value 'Bind IPv4/IPv6 address:port, default: 0.0.0.0:{RENDEZVOUS_PORT},[::]:{RENDEZVOUS_PORT} (dual stack)'
        -s, --serial=[NUMBER(default=0)] 'Sets configure update serial number'
        -R, --rendezvous-servers=[HOSTS] 'Sets rendezvous servers, separated by comma'
        -u, --software-url=[URL] 'Sets download url of RustDesk software of newest version'
        -r, --relay-servers=[HOST] 'Sets the default relay servers, separated by comma'
        -M, --rmem=[NUMBER(default={RMEM})] 'Sets UDP recv buffer size'
        , --mask=[MASK] 'Determine if the connection comes from LAN'
        -k, --key=[KEY] 'Only allow the client with the same key'",
    );
    init_args(&args, "hbbs", "RustDesk ID/Rendezvous Server");

    // 核心：默认双栈监听（0.0.0.0:21116 + [::]:21116），支持 -b 参数覆盖
    let default_bind = format!("0.0.0.0:{RENDEZVOUS_PORT},[::]:{RENDEZVOUS_PORT}");
    let bind_addrs_str = get_arg_or("bind", default_bind);
    let mut bind_addrs = Vec::new();
    
    // 解析 -b 参数（逗号分隔 IPv4/IPv6 地址）
    for addr_str in bind_addrs_str.split(',') {
        let addr = SocketAddr::from_str(addr_str.trim())
            .map_err(|e| bail!("Invalid bind address '{}': {}", addr_str, e))?;
        bind_addrs.push(addr);
    }

    // 兼容原有 -p 参数
    let port = get_arg_or("port", RENDEZVOUS_PORT.to_string()).parse::<u16>()?;
    if bind_addrs.is_empty() {
        bind_addrs.push(SocketAddr::from_str(&format!("0.0.0.0:{}", port))?);
    }

    let rmem = get_arg("rmem").parse::<usize>().unwrap_or(RMEM);
    let serial: i32 = get_arg("serial").parse().unwrap_or(0);
    crate::common::check_software_update();

    // 启动多地址监听
    RendezvousServer::start(&bind_addrs, serial, &get_arg_or("key", "-".to_owned()), rmem)?;
    Ok(())
}
