//! Packet captures: pcap and pcapng files, read down to TCP and UDP.

use crate::value::{Table, Value};
use indexmap::IndexMap;
use std::net::{Ipv4Addr, Ipv6Addr};

/// A capture's packets as a table of time, src, dst, proto, sport, dport, len and payload.
pub fn packets(b: &[u8]) -> Result<Table, String> {
    let mut rows = vec![];
    let mut add = |link: u32, ns: Option<i128>, len: usize, frame: &[u8]| {
        let time = ns.and_then(|ns| jiff::Timestamp::from_nanosecond(ns).ok()).map_or(Value::Nil, |t| Value::date(t.to_zoned(crate::host::tz())));
        let mut row = vec![time];
        row.extend(decode(link, frame));
        row.insert(6, Value::int(len as i64));
        rows.push(row);
    };
    let magic = b.get(..4).ok_or("not a pcap or pcapng file")?;
    if magic == b"\x0a\x0d\x0d\x0a" {
        pcapng(b, &mut add)?;
    } else {
        let le = match magic {
            [0xd4, 0xc3, 0xb2, 0xa1] | [0x4d, 0x3c, 0xb2, 0xa1] => true,
            [0xa1, 0xb2, 0xc3, 0xd4] | [0xa1, 0xb2, 0x3c, 0x4d] => false,
            _ => return Err("not a pcap or pcapng file".into()),
        };
        let nano = magic.contains(&0x3c);
        let u32_at = |at: usize| b.get(at..at + 4).map(|x| int(x, le) as u32);
        let link = u32_at(20).ok_or("truncated pcap header")?;
        let mut at = 24;
        while let (Some(sec), Some(frac), Some(cap), Some(len)) = (u32_at(at), u32_at(at + 4), u32_at(at + 8), u32_at(at + 12)) {
            let frame =
                b.get(at + 16..(at + 16).saturating_add(cap as usize)).ok_or_else(|| format!("the packet at byte {at} runs past the end of the file"))?;
            add(link, Some(sec as i128 * 1_000_000_000 + frac as i128 * if nano { 1 } else { 1000 }), len as usize, frame);
            at += 16 + cap as usize;
        }
    }
    let cols = ["time", "src", "dst", "proto", "sport", "dport", "len", "payload"];
    Ok(Table { cols: cols.map(String::from).to_vec(), rows })
}

fn int(x: &[u8], le: bool) -> u64 {
    if le { x.iter().rev().fold(0, |n, &c| n << 8 | c as u64) } else { x.iter().fold(0, |n, &c| n << 8 | c as u64) }
}

/// Each packet of a pcapng file, passed to `add` with its link type, time in nanoseconds, original length and bytes.
fn pcapng(b: &[u8], add: &mut impl FnMut(u32, Option<i128>, usize, &[u8])) -> Result<(), String> {
    // Interfaces as link type and nanoseconds per timestamp unit.
    let mut ifaces: Vec<(u32, f64)> = vec![];
    let mut le = true;
    let mut at = 0;
    while at + 12 <= b.len() {
        if b[at..at + 4] == *b"\x0a\x0d\x0d\x0a" {
            le = b.get(at + 8..at + 12) == Some(b"\x4d\x3c\x2b\x1a");
            ifaces.clear();
        }
        let u32_at = |i: usize| b.get(i..i + 4).map_or(0, |x| int(x, le) as usize);
        let (kind, size) = (u32_at(at), u32_at(at + 4));
        let body = b
            .get(at + 8..at.saturating_add(size).saturating_sub(4))
            .filter(|_| size >= 12)
            .ok_or_else(|| format!("the block at byte {at} runs past the end of the file"))?;
        match kind {
            1 => {
                let mut unit = 1000.0;
                // Options from byte 8: code, length, value padded to 4 bytes. if_tsresol (9) sets the timestamp unit.
                let mut o = 8;
                while let (Some(code), Some(len)) = (body.get(o..o + 2), body.get(o + 2..o + 4)) {
                    let (code, len) = (int(code, le), int(len, le) as usize);
                    if code == 0 {
                        break;
                    }
                    if let (9, Some(&r)) = (code, body.get(o + 4)) {
                        let exp = (r & 0x7f) as i32;
                        unit = 1e9 / if r & 0x80 == 0 { 10f64.powi(exp) } else { 2f64.powi(exp) };
                    }
                    o += 4 + len.div_ceil(4) * 4;
                }
                ifaces.push((b.get(at + 8..at + 10).map_or(0, |x| int(x, le) as u32), unit));
            }
            6 => {
                let &(link, unit) = ifaces.get(u32_at(at + 8)).ok_or_else(|| format!("the packet at byte {at} names an interface that wasn't described"))?;
                let ts = (u32_at(at + 12) as u64) << 32 | u32_at(at + 16) as u64;
                let frame = body.get(20..u32_at(at + 20).saturating_add(20)).ok_or_else(|| format!("the packet at byte {at} runs past its block"))?;
                add(link, Some((ts as f64 * unit) as i128), u32_at(at + 24), frame);
            }
            3 => {
                let link = ifaces.first().map_or(1, |i| i.0);
                let len = u32_at(at + 8);
                add(link, None, len, &body[4..body.len().min(len.saturating_add(4))]);
            }
            _ => {}
        }
        at += size;
    }
    Ok(())
}

/// A frame's src, dst, proto, sport, dport and payload, as far down as zil reads it.
fn decode(link: u32, f: &[u8]) -> Vec<Value> {
    let be16 = |x: &[u8], at: usize| x.get(at..at + 2).map_or(0, |v| u16::from_be_bytes([v[0], v[1]]));
    let (mut ether, mut at) = match link {
        1 => (be16(f, 12), 14),
        113 => (be16(f, 14), 16),
        276 => (be16(f, 0), 20),
        // BSD loopback: a 4-byte address family in host order. 2 is IPv4, and 24, 28 and 30 are IPv6.
        0 => (if f.first() == Some(&2) || f.get(3) == Some(&2) { 0x0800 } else { 0x86dd }, 4),
        101 | 12 | 228 | 229 => (if f.first().is_some_and(|v| v >> 4 == 6) { 0x86dd } else { 0x0800 }, 0),
        _ => (0, 0),
    };
    while ether == 0x8100 || ether == 0x88a8 {
        (ether, at) = (be16(f, at + 2), at + 4);
    }
    let p = f.get(at..).unwrap_or_default();
    let ip4 = |x: &[u8]| <[u8; 4]>::try_from(x).map_or(Value::Nil, |a| Value::str(Ipv4Addr::from(a).to_string()));
    let ip6 = |x: &[u8]| <[u8; 16]>::try_from(x).map_or(Value::Nil, |a| Value::str(Ipv6Addr::from(a).to_string()));
    let (src, dst, proto, rest) = match ether {
        0x0800 if p.len() >= 20 => {
            let end = (be16(p, 2) as usize).clamp(20, p.len());
            (ip4(&p[12..16]), ip4(&p[16..20]), p[9], p.get((p[0] as usize & 15) * 4..end).unwrap_or_default())
        }
        0x86dd if p.len() >= 40 => (ip6(&p[8..24]), ip6(&p[24..40]), p[6], &p[40..(40 + be16(p, 4) as usize).min(p.len())]),
        0x0806 if p.len() >= 28 => return vec![ip4(&p[14..18]), ip4(&p[24..28]), Value::str("ARP"), Value::Nil, Value::Nil, Value::bytes(p)],
        _ => return vec![Value::Nil, Value::Nil, Value::str(format!("0x{ether:04x}")), Value::Nil, Value::Nil, Value::bytes(p)],
    };
    let port = |at: usize| Value::int(be16(rest, at) as i64);
    let (name, ports, payload) = match proto {
        6 if rest.len() >= 20 => ("TCP", Some((port(0), port(2))), rest.get((rest[12] >> 4) as usize * 4..).unwrap_or_default()),
        17 if rest.len() >= 8 => ("UDP", Some((port(0), port(2))), &rest[8..]),
        1 => ("ICMP", None, rest),
        58 => ("ICMPv6", None, rest),
        _ => ("", None, rest),
    };
    let name = if name.is_empty() { Value::str(format!("IP proto {proto}")) } else { Value::str(name) };
    let (sport, dport) = ports.unwrap_or((Value::Nil, Value::Nil));
    vec![src, dst, name, sport, dport, Value::bytes(payload)]
}

/// TCP payloads joined per connection, in capture order, as a table of src, dst, packets and data.
pub fn streams(t: &Table) -> Result<Table, String> {
    let col = |name: &str| t.cols.iter().position(|c| c == name).ok_or_else(|| format!("expected a table from pcap, with a {name} column"));
    let [src, dst, proto, sport, dport, payload] = ["src", "dst", "proto", "sport", "dport", "payload"].map(col);
    let (src, dst, proto, sport, dport, payload) = (src?, dst?, proto?, sport?, dport?, payload?);
    // Each connection, keyed by its sorted endpoints, as its first packet's endpoints, a packet count and the data.
    let mut conns: IndexMap<[String; 2], ([String; 2], i64, Vec<u8>)> = IndexMap::new();
    for r in t.rows.iter().filter(|r| r[proto].to_string() == "TCP") {
        let ends = [format!("{}:{}", r[src], r[sport]), format!("{}:{}", r[dst], r[dport])];
        let mut key = ends.clone();
        key.sort();
        let c = conns.entry(key).or_insert_with(|| (ends, 0, vec![]));
        c.1 += 1;
        if let Value::Bytes(p) = &r[payload] {
            c.2.extend_from_slice(p);
        }
    }
    let rows = conns.into_values().map(|([a, b], n, data)| vec![Value::str(a), Value::str(b), Value::int(n), Value::bytes(data)]).collect();
    Ok(Table { cols: ["src", "dst", "packets", "data"].map(String::from).to_vec(), rows })
}
