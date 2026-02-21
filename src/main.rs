use clap::Parser;
use smoltcp::wire::{Ipv4Address, Ipv4Packet};
use std::io::{Read, Write};
use tun::AbstractDevice;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the TUN interface
    #[arg(short, long, default_value = "sidestore")]
    tun_name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Set up Ctrl+C handler to exit immediately
    ctrlc::set_handler(|| {
        std::process::exit(0);
    })?;

    let mut config = tun::Configuration::default();
    config.tun_name(&args.tun_name);
    config.up();

    let mut dev = tun::create(&config)?;
    dev.set_address(std::net::IpAddr::V4(Ipv4Address::new(10, 7, 0, 0)))
        .expect("Failed to set interface address");
    dev.set_destination(std::net::IpAddr::V4(Ipv4Address::new(10, 7, 0, 1)))
        .expect("Failed to set destination address");
    dev.enabled(true).expect("Failed to enable interface");

    println!("TUN device \"{}\" is up", args.tun_name);

    let mut buf = [0u8; 1504]; // MTU of 1500 + 4 bytes for header

    loop {
        let n = dev.read(&mut buf)?;
        let packet_buf = &mut buf[..n];

        // Parse the packet as an IPv4 packet.
        if let Ok(mut ipv4_packet) = Ipv4Packet::new_checked(packet_buf) {
            let dst_addr = ipv4_packet.dst_addr();

            // Check if the destination address is 10.7.0.1
            if dst_addr == Ipv4Address::new(10, 7, 0, 1) {
                // Swap source and destination addresses
                let src_addr = ipv4_packet.src_addr();
                ipv4_packet.set_dst_addr(src_addr);
                ipv4_packet.set_src_addr(dst_addr);

                // The checksum remains valid: swapping src and dst preserves the
                // one's complement sum over the header words.
                dev.write(ipv4_packet.into_inner())?;
            }
        }
        // Other packets are dropped.
    }
}
