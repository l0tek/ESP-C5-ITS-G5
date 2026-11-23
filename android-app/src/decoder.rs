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

fn infrastructure(
    geonetworking: Option<c_its_parser::gn::Packet<'_>>,
    station_id: u32,
    kind: &str,
    received_ms: u64,
    details: Vec<String>,
) -> Result<Observation, String> {
    let position = geonetworking
        .and_then(|packet| packet.source_position_vector())
        .ok_or("GeoNetworking-Quellposition fehlt")?;
    let (latitude, longitude) = coordinates(position.latitude, position.longitude)?;
    Ok(Observation {
        key: format!("{}:{station_id}", kind.to_ascii_lowercase()),
        station_id,
        kind: kind.into(),
        latitude,
        longitude,
        speed_kmh: None,
        heading: None,
        station_type: 0,
        terminated: false,
        received_ms,
        details,
    })
}

use c_its_parser::standards::dsrc_2_2_1::etsi_its_dsrc::{
    IntersectionAccessPoint, MovementPhaseState, PriorityRequestType, VehicleID,
};

fn phase_name(state: MovementPhaseState) -> &'static str {
    match state {
        MovementPhaseState::unavailable => "unbekannt",
        MovementPhaseState::dark => "dunkel",
        MovementPhaseState::stop_Then_Proceed => "rot blinkend (Halt, dann fahren)",
        MovementPhaseState::stop_And_Remain => "rot",
        MovementPhaseState::pre_Movement => "rot-gelb",
        MovementPhaseState::permissive_Movement_Allowed => "grün (bedingt)",
        MovementPhaseState::protected_Movement_Allowed => "grün (geschützt)",
        MovementPhaseState::permissive_clearance => "gelb (bedingt)",
        MovementPhaseState::protected_clearance => "gelb (geschützt)",
        MovementPhaseState::caution_Conflicting_Traffic => "gelb blinkend",
    }
}

fn access_point(point: &IntersectionAccessPoint) -> String {
    match point {
        IntersectionAccessPoint::lane(id) => format!("Spur {}", id.0),
        IntersectionAccessPoint::approach(id) => format!("Zufahrt {}", id.0),
        IntersectionAccessPoint::connection(id) => format!("Verbindung {}", id.0),
        _ => "unbekannter Zugangspunkt".into(),
    }
}

fn request_type(value: PriorityRequestType) -> &'static str {
    match value {
        PriorityRequestType::priorityRequestTypeReserved => "reserviert",
        PriorityRequestType::priorityRequest => "Prioritätsanforderung",
        PriorityRequestType::priorityRequestUpdate => "Aktualisierung der Prioritätsanforderung",
        PriorityRequestType::priorityCancellation => "Stornierung der Prioritätsanforderung",
        _ => "unbekannte Anforderung",
    }
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
        details: Vec::new(),
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
        ItsMessage::Spatem {
            geonetworking,
            etsi,
            ..
        } => {
            let mut details = Vec::new();
            for intersection in &etsi.spat.intersections.0 {
                for movement in &intersection.states.0 {
                    for event in &movement.state_time_speed.0 {
                        let mut text = format!(
                            "Kreuzung {} · Signalgruppe {}: {}",
                            intersection.id.id.0,
                            movement.signal_group.0,
                            phase_name(event.event_state)
                        );
                        if let Some(timing) = &event.timing {
                            if timing.min_end_time.0 != 36001 {
                                text.push_str(&format!(
                                    " · frühester Wechsel bei {:.1} s in der UTC-Stunde",
                                    f64::from(timing.min_end_time.0) / 10.0
                                ));
                            }
                            if let Some(max) = &timing.max_end_time
                                && max.0 != 36001
                            {
                                text.push_str(&format!(
                                    " (spätestens {:.1} s)",
                                    f64::from(max.0) / 10.0
                                ));
                            }
                        }
                        details.push(text);
                    }
                }
            }
            return infrastructure(
                geonetworking,
                etsi.header.station_id.0,
                "SPATEM",
                received_ms,
                details,
            );
        }
        ItsMessage::Mapem {
            geonetworking,
            etsi,
            ..
        } => {
            let mut details = Vec::new();
            if let Some(intersections) = &etsi.map.intersections {
                for intersection in &intersections.0 {
                    for lane in &intersection.lane_set.0 {
                        if let Some(connections) = &lane.connects_to {
                            for connection in &connections.0 {
                                if let Some(group) = &connection.signal_group {
                                    details.push(format!(
                                        "Kreuzung {} · Spur {} → Spur {} · Signalgruppe {}",
                                        intersection.id.id.0,
                                        lane.lane_id.0,
                                        connection.connecting_lane.lane.0,
                                        group.0
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            return infrastructure(
                geonetworking,
                etsi.header.station_id.0,
                "MAPEM",
                received_ms,
                details,
            );
        }
        ItsMessage::Srem {
            geonetworking,
            etsi,
            ..
        } => {
            let requester = match &etsi.srm.requestor.id {
                VehicleID::stationID(id) => format!("ITS-Station {}", id.0),
                VehicleID::entityID(id) => format!("temporäre ID {:02X?}", id.0.as_ref()),
            };
            let mut details = vec![format!(
                "Absender: {requester} · ITS-PDU-Station {}",
                etsi.header.station_id.0
            )];
            if let Some(requests) = &etsi.srm.requests {
                for package in &requests.0 {
                    let request = &package.request;
                    let mut text = format!(
                        "{} {} für Kreuzung {}: {}",
                        request_type(request.request_type),
                        request.request_id.0,
                        request.id.id.0,
                        access_point(&request.in_bound_lane)
                    );
                    if let Some(out) = &request.out_bound_lane {
                        text.push_str(&format!(" → {}", access_point(out)));
                    }
                    if let Some(second) = &package.second {
                        text.push_str(&format!(
                            " · Ankunft {:.3} s in der Minute",
                            f64::from(second.0) / 1000.0
                        ));
                    }
                    if let Some(duration) = &package.duration {
                        text.push_str(&format!(" · Dauer {:.3} s", f64::from(duration.0) / 1000.0));
                    }
                    details.push(text);
                }
            }
            return infrastructure(
                geonetworking,
                etsi.header.station_id.0,
                "SREM",
                received_ms,
                details,
            );
        }
    }
    Ok(out)
}
