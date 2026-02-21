use clap::Parser;
use smoltcp::wire::{IpProtocol, Ipv4Address, Ipv4Packet, TcpPacket};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::{Duration, Instant};
use tun::AbstractDevice;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the TUN interface
    #[arg(short, long, default_value = "sidestore")]
    tun_name: String,
}

const CONTROL_PORT: u16 = 62078;
const PAIR_OPEN_WINDOW_AFTER_CONTROL: Duration = Duration::from_secs(5);
const CONTROL_EVENT_TTL: Duration = Duration::from_secs(30);
const DATA_PAIR_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PairKey {
    client: String,
    low: u16,
    high: u16,
}

#[derive(Debug, Clone, Copy)]
struct PairState {
    last_seen: Instant,
}

#[derive(Debug, Default)]
struct SessionTracker {
    last_control_syn: HashMap<String, Instant>,
    data_pairs: HashMap<PairKey, PairState>,
    last_cleanup: Option<Instant>,
}

impl SessionTracker {
    fn pair_key(client: &str, a: u16, b: u16) -> PairKey {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        PairKey {
            client: client.to_string(),
            low,
            high,
        }
    }

    fn note_control_syn(&mut self, client: &str, now: Instant) {
        self.last_control_syn.insert(client.to_string(), now);
    }

    fn can_open_pair(&self, client: &str, now: Instant) -> bool {
        self.last_control_syn
            .get(client)
            .is_some_and(|seen| now.duration_since(*seen) <= PAIR_OPEN_WINDOW_AFTER_CONTROL)
    }

    fn has_pair(&self, key: &PairKey) -> bool {
        self.data_pairs.contains_key(key)
    }

    fn touch_pair(&mut self, key: &PairKey, now: Instant) {
        if let Some(state) = self.data_pairs.get_mut(key) {
            state.last_seen = now;
        }
    }

    fn open_pair(&mut self, key: PairKey, now: Instant) {
        self.data_pairs.insert(key, PairState { last_seen: now });
    }

    fn close_pair(&mut self, key: &PairKey) {
        self.data_pairs.remove(key);
    }

    fn cleanup(&mut self, now: Instant) {
        if let Some(last) = self.last_cleanup
            && now.duration_since(last) < CLEANUP_INTERVAL
        {
            return;
        }

        self.last_control_syn
            .retain(|_, seen| now.duration_since(*seen) <= CONTROL_EVENT_TTL);
        self.data_pairs
            .retain(|_, state| now.duration_since(state.last_seen) <= DATA_PAIR_IDLE_TIMEOUT);
        self.last_cleanup = Some(now);
    }
}

fn is_syn_only<T: AsRef<[u8]>>(tcp: &TcpPacket<T>) -> bool {
    tcp.syn() && !tcp.ack()
}

fn is_pair_candidate(src_port: u16, dst_port: u16) -> bool {
    let (low, high) = if src_port <= dst_port {
        (src_port, dst_port)
    } else {
        (dst_port, src_port)
    };
    low != CONTROL_PORT && high - low == 1
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut tracker = SessionTracker::default();

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
            let src_addr = ipv4_packet.src_addr();

            // Check if the destination address is 10.7.0.1
            if dst_addr != Ipv4Address::new(10, 7, 0, 1) {
                continue;
            }

            if ipv4_packet.next_header() != IpProtocol::Tcp {
                continue;
            }

            let tcp = match TcpPacket::new_checked(ipv4_packet.payload_mut()) {
                Ok(tcp) => tcp,
                Err(_) => continue,
            };

            let now = Instant::now();
            tracker.cleanup(now);

            let src_ip = src_addr.to_string();
            let src_port = tcp.src_port();
            let dst_port = tcp.dst_port();
            let syn_only = is_syn_only(&tcp);
            let fin_or_rst = tcp.fin() || tcp.rst();
            let mut should_forward = false;

            if dst_port == CONTROL_PORT || src_port == CONTROL_PORT {
                if syn_only && dst_port == CONTROL_PORT {
                    tracker.note_control_syn(&src_ip, now);
                }
                should_forward = true;
            } else {
                let key = SessionTracker::pair_key(&src_ip, src_port, dst_port);

                if tracker.has_pair(&key) {
                    tracker.touch_pair(&key, now);
                    if fin_or_rst {
                        tracker.close_pair(&key);
                    }
                    should_forward = true;
                } else if syn_only
                    && is_pair_candidate(src_port, dst_port)
                    && tracker.can_open_pair(&src_ip, now)
                {
                    tracker.open_pair(key, now);
                    should_forward = true;
                }
            }

            if should_forward {
                // Swap source and destination addresses
                ipv4_packet.set_dst_addr(src_addr);
                ipv4_packet.set_src_addr(dst_addr);

                // The checksum is automatically updated by the setters.
                dev.write(ipv4_packet.into_inner())?;
            }
        }
        // Other packets are dropped.
    }
}
