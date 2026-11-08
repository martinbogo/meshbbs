//! Wire-compatibility checks across Meshtastic protobuf revisions.
//!
//! meshbbs must keep working with radios whose firmware is older or newer than the protobuf
//! schema it was compiled against. These tests pin the properties that guarantee this:
//!
//! * Outbound packets built with `..Default::default()` encode only the fields we set, so
//!   fields added by newer schemas (e.g. `Data.xeddsa_signature`) or whose Rust type changed
//!   (e.g. `MeshPacket.rx_time`, `u32` -> `Option<u32>`) never reach older firmware.
//! * Messages carrying fields or oneof variants unknown to the compiled schema (sent by newer
//!   firmware) still decode, with the unknown parts skipped.
#![cfg(feature = "meshtastic-proto")]

use meshbbs::protobuf::meshtastic_generated as proto;
use prost::Message;

/// Decode a varint at `buf[*i..]`, advancing `i`.
fn read_varint(buf: &[u8], i: &mut usize) -> u64 {
    let mut out = 0u64;
    let mut shift = 0;
    loop {
        let b = buf[*i];
        *i += 1;
        out |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return out;
        }
        shift += 7;
    }
}

/// Field numbers present at the top level of an encoded message.
fn top_level_fields(buf: &[u8]) -> Vec<u32> {
    let mut fields = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        let key = read_varint(buf, &mut i);
        fields.push((key >> 3) as u32);
        match key & 7 {
            0 => {
                read_varint(buf, &mut i);
            }
            1 => i += 8,
            2 => {
                let len = read_varint(buf, &mut i) as usize;
                i += len;
            }
            5 => i += 4,
            wt => panic!("unexpected wire type {wt}"),
        }
    }
    fields
}

/// Append a length-delimited field with an arbitrary (unknown) field number.
fn push_unknown_len_field(buf: &mut Vec<u8>, field: u32, body: &[u8]) {
    prost::encoding::encode_key(field, prost::encoding::WireType::LengthDelimited, buf);
    prost::encoding::encode_varint(body.len() as u64, buf);
    buf.extend_from_slice(body);
}

fn outbound_text_packet() -> proto::MeshPacket {
    // Mirrors how src/meshtastic builds outbound text packets.
    let data = proto::Data {
        portnum: proto::PortNum::TextMessageApp as i32,
        payload: b"hello".to_vec().into(),
        ..Default::default()
    };
    proto::MeshPacket {
        from: 0x1234_5678,
        to: 0xffff_ffff,
        channel: 0,
        payload_variant: Some(proto::mesh_packet::PayloadVariant::Decoded(data)),
        id: 42,
        hop_limit: 3,
        want_ack: true,
        priority: 70,
        ..Default::default()
    }
}

#[test]
fn outbound_data_encodes_only_portnum_and_payload() {
    let pkt = outbound_text_packet();
    let Some(proto::mesh_packet::PayloadVariant::Decoded(data)) = &pkt.payload_variant else {
        unreachable!()
    };
    // Data: portnum = 1, payload = 2. Nothing else (in particular no xeddsa_signature = 10).
    assert_eq!(top_level_fields(&data.encode_to_vec()), vec![1, 2]);
}

#[test]
fn outbound_mesh_packet_encodes_only_fields_we_set() {
    let fields = top_level_fields(&outbound_text_packet().encode_to_vec());
    // from = 1, to = 2, decoded = 4, id = 6, hop_limit = 9, want_ack = 10, priority = 11.
    // channel is 0 and so omitted; rx_time (7) and rx_snr (8) must never be sent.
    assert_eq!(fields, vec![1, 2, 4, 6, 9, 10, 11]);
}

#[test]
fn from_radio_with_unknown_variant_decodes_as_empty() {
    // A future firmware adds a FromRadio oneof variant this build does not know.
    let mut buf = Vec::new();
    prost::encoding::uint32::encode(1, &7, &mut buf); // id = 7
    push_unknown_len_field(&mut buf, 9_999, b"\x08\x01");
    let msg = proto::FromRadio::decode(buf.as_slice()).expect("unknown variant must not fail");
    assert_eq!(msg.id, 7);
    assert!(msg.payload_variant.is_none());
}

#[test]
fn mesh_packet_with_unknown_fields_keeps_known_payload() {
    let mut buf = outbound_text_packet().encode_to_vec();
    push_unknown_len_field(&mut buf, 9_998, b"future");
    let pkt = proto::MeshPacket::decode(buf.as_slice()).expect("unknown field must not fail");
    assert_eq!(pkt.id, 42);
    match pkt.payload_variant {
        Some(proto::mesh_packet::PayloadVariant::Decoded(d)) => {
            assert_eq!(&d.payload[..], b"hello");
        }
        other => panic!("expected decoded payload, got {other:?}"),
    }
}

#[test]
fn unknown_enum_values_are_preserved_as_raw_integers() {
    // Newer firmware may send port numbers or routing errors this build has no name for.
    let data = proto::Data {
        portnum: 4_000,
        ..Default::default()
    };
    let back = proto::Data::decode(data.encode_to_vec().as_slice()).unwrap();
    assert_eq!(back.portnum, 4_000);
    assert!(proto::PortNum::try_from(back.portnum).is_err());
}
