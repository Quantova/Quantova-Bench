//! The post quantum transport cost, measured on the real qtv-net channel.

use std::thread;
use std::time::Instant;

use qtv_net::{duplex, Channel, Identity};

/// Megabytes per second a real post quantum channel seals and opens a payload
pub fn seal_open_mbps(payload: &[u8], reps: u32) -> f64 {
    let initiator = Identity::from_seed(&[161u8; 32]);
    let responder = Identity::from_seed(&[178u8; 32]);
    let (near, far) = duplex();

    let responder_side = thread::spawn(move || Channel::accept(far, &responder).expect("accept"));
    let mut client = Channel::connect(near, &initiator).expect("connect");
    let mut server = responder_side.join().expect("handshake");

    client.send(payload).expect("warm send");
    let _ = server.recv().expect("warm recv");

    let start = Instant::now();
    for _ in 0..reps {
        client.send(payload).expect("send");
        let got = server.recv().expect("recv");
        std::hint::black_box(got.len());
    }
    let secs = start.elapsed().as_secs_f64();
    (payload.len() as f64 * reps as f64 / secs) / 1e6
}
