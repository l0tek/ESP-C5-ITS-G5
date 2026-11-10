use crate::model::Observation;
use c_its_parser::{Headers, ItsMessage, de};

/// Strip only validated, unfragmented 802.11 data + LLC/SNAP headers.
pub fn geonetworking_payload(frame: &[u8]) -> Result<&[u8], String> {
    if !(32..=crate::stream::MAX_FRAME).contains(&frame.len()) {
        return Err("Frame zu kurz/lang".into());
    }
    let fc = u16::from_le_bytes([frame[0], frame[1]]);
    let subtype = (fc >> 4) & 15;
    if fc & 15 != 8 || ![0, 8].contains(&subtype) {
        return Err("Kein unterstützter Datenframe".into());
    }
    if fc & 0x4300 != 0 || fc & 0x0400 != 0 || frame[22] & 15 != 0 {
        return Err("DS-, verschlüsselte oder fragmentierte Frames nicht unterstützt".into());
    }
    let header_len = 24
        + if subtype == 8 { 2 } else { 0 }
        + if subtype == 8 && fc & 0x8000 != 0 {
            4
        } else {
            0
        };
    if subtype == 8 && frame[24] & 0x80 != 0 {
        return Err("A-MSDU nicht unterstützt".into());
    }
    let llc = frame
        .get(header_len..header_len + 8)
        .ok_or("LLC-Header fehlt")?;
    if llc != [0xaa, 0xaa, 3, 0, 0, 0, 0x89, 0x47] {
        return Err("Kein GeoNetworking-LLC/SNAP".into());
    }
    let payload = &frame[header_len + 8..];
    if payload.len() < 12 {
        return Err("GeoNetworking-Header abgeschnitten".into());
    }
    Ok(payload)
}

pub fn decode_frame(frame: &[u8], received_ms: u64) -> Result<Observation, String> {
    let payload = geonetworking_payload(frame)?;
    // The upstream GN parser has unchecked slices for some malformed secured packets.
    // Isolate these failures at the untrusted radio-data boundary.
    std::panic::catch_unwind(|| {
        let message = de::decode(payload, Headers::GnBtp)?;
        observation(message, received_ms)
    })
    .map_err(|_| "Parser hat beschädigtes Paket verworfen".to_string())?
}

fn coordinates(lat: i32, lon: i32) -> Result<(f64, f64), String> {
    if !(-900_000_000..=900_000_000).contains(&lat)
        || !(-1_800_000_000..=1_800_000_000).contains(&lon)
    {
        return Err("Position nicht verfügbar oder außerhalb WGS84".into());
    }
    Ok((f64::from(lat) / 10_000_000., f64::from(lon) / 10_000_000.))
}
fn observation(message: ItsMessage<'_>, received_ms: u64) -> Result<Observation, String> {
    use c_its_parser::standards::cam_1_4_1::cam_pdu_descriptions::HighFrequencyContainer;
    let mut out = Observation {
        key: String::new(),
        station_id: 0,
        kind: String::new(),
        latitude: 0.,
        longitude: 0.,
        speed_kmh: None,
        heading: None,
        station_type: 0,
        terminated: false,
        received_ms,
    };
    match message {
        ItsMessage::Cam { etsi, .. } => {
            out.station_id = etsi.header.station_id.0;
            out.key = format!("cam:{}", out.station_id);
            out.kind = "CAM".into();
            let params = &etsi.cam.cam_parameters;
            out.station_type = params.basic_container.station_type.0;
            let pos = &params.basic_container.reference_position;
            (out.latitude, out.longitude) = coordinates(pos.latitude.0, pos.longitude.0)?;
            if let HighFrequencyContainer::basicVehicleContainerHighFrequency(hf) =
                &params.high_frequency_container
            {
                if hf.speed.speed_value.0 < 16383 {
                    out.speed_kmh = Some(f64::from(hf.speed.speed_value.0) * 0.036);
                }
                if hf.heading.heading_value.0 <= 3600 {
                    out.heading = Some(f64::from(hf.heading.heading_value.0) / 10.);
                }
            }
        }
        ItsMessage::DenmV1 { etsi, .. } => {
            let m = &etsi.denm.management;
            out.station_id = m.action_id.originating_station_id.0;
            out.key = format!("denm:{}:{}", out.station_id, m.action_id.sequence_number.0);
            out.kind = "DENM".into();
            out.station_type = m.station_type.0;
            out.terminated = m.termination.is_some();
            if !out.terminated {
                (out.latitude, out.longitude) =
                    coordinates(m.event_position.latitude.0, m.event_position.longitude.0)?;
            }
        }
        ItsMessage::DenmV2 { etsi, .. } => {
            let m = &etsi.denm.management;
            out.station_id = m.action_id.originating_station_id.0;
            out.key = format!("denm:{}:{}", out.station_id, m.action_id.sequence_number.0);
            out.kind = "DENM".into();
            out.station_type = m.station_type.0;
            out.terminated = m.termination.is_some();
            if !out.terminated {
                (out.latitude, out.longitude) =
                    coordinates(m.event_position.latitude.0, m.event_position.longitude.0)?;
            }
        }
    }
    Ok(out)
}
