use cits_map::{
    decoder::decode_frame,
    demo,
    forwarding::packet_topic,
    model::{Dashboard, MAX_OBJECTS},
    runtime::Runtime,
    stream::{self, LineDecoder},
};

#[test]
fn cam_radio_frame_to_map_position() {
    let frame = demo::cam_frame(1234, 535_510_860, 99_937_000);
    let o = decode_frame(&frame, 10).unwrap();
    assert_eq!(o.station_id, 1234);
    assert_eq!(o.kind, "CAM");
    assert!((o.latitude - 53.551086).abs() < 1e-7);
    assert!((o.longitude - 9.9937).abs() < 1e-7);
    assert!(o.speed_kmh.is_none());
}
#[test]
fn qos_header_is_supported() {
    let mut frame = demo::cam_frame(7, 0, 0);
    frame[0] = 0x88;
    frame.splice(24..24, [0, 0]);
    assert_eq!(decode_frame(&frame, 1).unwrap().station_id, 7);
}
#[test]
fn unavailable_position_rejected() {
    assert!(decode_frame(&demo::cam_frame(1, 900_000_001, 0), 0).is_err());
}
#[test]
fn malformed_frames_do_not_panic() {
    for n in 0..128 {
        assert!(decode_frame(&vec![0; n], 0).is_err());
    }
    let valid = demo::cam_frame(1, 0, 0);
    for n in 0..valid.len() {
        let _ = decode_frame(&valid[..n], 0);
    }
    for offset in 0..valid.len() {
        let mut damaged = valid.clone();
        damaged[offset] ^= 0xff;
        assert!(std::panic::catch_unwind(|| decode_frame(&damaged, 0)).is_ok());
    }
}
#[test]
fn encrypted_and_fragmented_frames_rejected() {
    let mut frame = demo::cam_frame(1, 0, 0);
    frame[1] = 0x40;
    assert!(decode_frame(&frame, 0).is_err());
    frame[1] = 0x04;
    assert!(decode_frame(&frame, 0).is_err());
    frame[1] = 0;
    frame[22] = 1;
    assert!(decode_frame(&frame, 0).is_err());
}
#[test]
fn usb_chunks_and_crlf_are_reassembled() {
    let frame = demo::cam_frame(1, 0, 0);
    let line = format!("boot\r\nCITS:{}\r\nCITS-STAT packets=1\n", hex(&frame));
    let mut stream = LineDecoder::default();
    let mut out = Vec::new();
    for chunk in line.as_bytes().chunks(7) {
        out.extend(stream.feed(chunk));
    }
    assert_eq!(out, vec![frame]);
    assert_eq!(stream.errors, 0);
}
#[test]
fn oversized_line_is_discarded_until_newline() {
    let mut stream = LineDecoder::default();
    assert!(stream.feed(&vec![b'a'; 50_000]).is_empty());
    let line = format!("\nCITS:{}\n", hex(&demo::cam_frame(1, 0, 0)));
    assert_eq!(stream.feed(line.as_bytes()).len(), 1);
    assert_eq!(stream.errors, 1);
}
#[test]
fn invalid_hex_and_lengths_are_counted() {
    let mut stream = LineDecoder::default();
    stream.feed(b"CITS:zz\nCITS:0\nCITS:\n");
    assert_eq!(stream.errors, 3);
}
#[test]
fn import_without_final_newline() {
    let frame = demo::cam_frame(1, 0, 0);
    assert_eq!(
        stream::import_frames(format!("CITS:{}", hex(&frame)).as_bytes())
            .unwrap()
            .0,
        vec![frame]
    );
}
#[test]
fn pcap_both_endian_orders() {
    let frame = demo::cam_frame(9, 0, 0);
    for le in [true, false] {
        assert_eq!(
            stream::import_frames(&pcap(&frame, le)).unwrap().0,
            vec![frame.clone()]
        );
    }
}
#[test]
fn pcap_truncation_wrong_link_and_pcapng_rejected() {
    let mut bytes = pcap(&demo::cam_frame(1, 0, 0), true);
    assert!(stream::import_frames(&bytes[..bytes.len() - 1]).is_err());
    bytes[20] = 1;
    assert!(stream::import_frames(&bytes).is_err());
    assert!(stream::import_frames(&[0x0a, 0x0d, 0x0d, 0x0a]).is_err());
}
#[test]
fn repeated_station_updates_and_expires() {
    let mut d = Dashboard::default();
    d.insert(decode_frame(&demo::cam_frame(1, 0, 0), 1).unwrap());
    d.insert(decode_frame(&demo::cam_frame(1, 10_000_000, 0), 10).unwrap());
    assert_eq!(d.objects.len(), 1);
    assert_eq!(d.objects["cam:1"].latitude, 1.);
    d.expire(60_010);
    assert!(d.objects.is_empty());
}
#[test]
fn station_memory_is_bounded() {
    let mut d = Dashboard::default();
    for station in 0..MAX_OBJECTS + 50 {
        d.insert(decode_frame(&demo::cam_frame(station as u32, 0, 0), station as u64).unwrap());
    }
    assert_eq!(d.objects.len(), MAX_OBJECTS);
    assert!(!d.objects.contains_key("cam:0"));
}
#[test]
fn mqtt_topic_cannot_inject_wildcards() {
    assert_eq!(
        packet_topic("3c:dc:75:8a:fb:68").unwrap(),
        "its/3c:dc:75:8a:fb:68/packet"
    );
    for bad in ["", "#", "+", "foo/bar", "node name", "ä"] {
        assert!(packet_topic(bad).is_err());
    }
}
#[test]
fn runtime_demo_and_import_use_real_decoding() {
    let rt = Runtime::default();
    rt.demo().unwrap();
    assert_eq!(rt.snapshot().decoded, 3);
    assert!(rt.forwarder.lock().unwrap().is_none());
    let frame = demo::cam_frame(9, 0, 0);
    rt.import(&pcap(&frame, true), "test").unwrap();
    assert_eq!(rt.snapshot().decoded, 1);
    assert_eq!(rt.snapshot().objects.len(), 1);
    assert!(rt.forwarder.lock().unwrap().is_none());
}
#[test]
fn denm_decodes_and_cancellation_removes_only_matching_action() {
    use c_its_parser::standards::{
        cdd_1_3_1_1::its_container::*, denm_1_3_1::denm_pdu_descriptions::*,
    };
    use c_its_parser::{EncodingRules, ItsMessage};
    let encode = |seq, termination| {
        let pos = ReferencePosition::new(
            Latitude(535_510_860),
            Longitude(99_937_000),
            PosConfidenceEllipse::new(
                SemiAxisLength(4095),
                SemiAxisLength(4095),
                HeadingValue(3601),
            ),
            Altitude::new(AltitudeValue(800001), AltitudeConfidence::unavailable),
        );
        let m = ManagementContainer::new(
            ActionID::new(StationID(42), SequenceNumber(seq)),
            TimestampIts(1),
            TimestampIts(1),
            termination,
            pos,
            None,
            None,
            ValidityDuration(60),
            None,
            StationType(5),
        );
        let etsi = DENM::new(
            ItsPduHeader::new(1, 1, StationID(42)),
            DecentralizedEnvironmentalNotificationMessage::new(m, None, None, None),
        );
        let bytes = ItsMessage::DenmV1 {
            geonetworking: None,
            transport: None,
            etsi: Box::new(etsi),
        }
        .encode(EncodingRules::UPER)
        .unwrap();
        demo::wrap_uper(&bytes, 2002)
    };
    let mut d = Dashboard::default();
    d.insert(decode_frame(&encode(1, None), 1).unwrap());
    d.insert(decode_frame(&encode(2, None), 1).unwrap());
    assert_eq!(d.objects.len(), 2);
    d.insert(decode_frame(&encode(1, Some(Termination::isCancellation)), 2).unwrap());
    assert_eq!(d.objects.len(), 1);
    assert!(d.objects.contains_key("denm:42:2"));
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn pcap(frame: &[u8], le: bool) -> Vec<u8> {
    let mut out = if le {
        vec![0xd4, 0xc3, 0xb2, 0xa1, 2, 0, 4, 0]
    } else {
        vec![0xa1, 0xb2, 0xc3, 0xd4, 0, 2, 0, 4]
    };
    let word = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
    for v in [
        0,
        0,
        65535,
        105,
        1,
        0,
        frame.len() as u32,
        frame.len() as u32,
    ] {
        out.extend_from_slice(&word(v));
    }
    out.extend_from_slice(frame);
    out
}

#[test]
fn cam_speed_heading_and_unavailable_values() {
    use c_its_parser::standards::{
        cam_1_4_1::cam_pdu_descriptions::*, cdd_1_3_1_1::its_container::*,
    };
    use c_its_parser::{EncodingRules, Headers, ItsMessage};
    let base = demo::cam_frame(50, 0, 0);
    let encode = |speed, heading| {
        let ItsMessage::Cam { mut etsi, .. } = c_its_parser::de::decode(
            cits_map::decoder::geonetworking_payload(&base).unwrap(),
            Headers::GnBtp,
        )
        .unwrap() else {
            panic!()
        };
        etsi.cam.cam_parameters.high_frequency_container =
            HighFrequencyContainer::basicVehicleContainerHighFrequency(
                BasicVehicleContainerHighFrequency::new(
                    Heading::new(HeadingValue(heading), HeadingConfidence(127)),
                    Speed::new(SpeedValue(speed), SpeedConfidence(127)),
                    DriveDirection::forward,
                    VehicleLength::new(
                        VehicleLengthValue(45),
                        VehicleLengthConfidenceIndication::noTrailerPresent,
                    ),
                    VehicleWidth(18),
                    LongitudinalAcceleration::new(
                        LongitudinalAccelerationValue(161),
                        AccelerationConfidence(102),
                    ),
                    Curvature::new(CurvatureValue(1023), CurvatureConfidence::unavailable),
                    CurvatureCalculationMode::unavailable,
                    YawRate::new(YawRateValue(32767), YawRateConfidence::unavailable),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            );
        let raw = ItsMessage::Cam {
            geonetworking: None,
            transport: None,
            etsi,
        }
        .encode(EncodingRules::UPER)
        .unwrap();
        demo::wrap_uper(&raw, 2001)
    };
    let item = decode_frame(&encode(1000, 900), 0).unwrap();
    assert_eq!(item.speed_kmh, Some(36.));
    assert_eq!(item.heading, Some(90.));
    let item = decode_frame(&encode(16383, 3601), 0).unwrap();
    assert_eq!(item.speed_kmh, None);
    assert_eq!(item.heading, None);
}

#[test]
fn denm_v2_event_position() {
    use c_its_parser::standards::{
        cdd_2_2_1::etsi_its_cdd::*, denm_2_2_1::denm_pdu_description::*,
    };
    use c_its_parser::{EncodingRules, ItsMessage};
    let pos = ReferencePosition::new(
        Latitude(535_510_860),
        Longitude(99_937_000),
        PosConfidenceEllipse::new(
            SemiAxisLength(4095),
            SemiAxisLength(4095),
            HeadingValue(3601),
        ),
        Altitude::new(AltitudeValue(800001), AltitudeConfidence::unavailable),
    );
    let m = ManagementContainer::new(
        ActionId::new(StationId(73), SequenceNumber(8)),
        TimestampIts(1),
        TimestampIts(1),
        None,
        pos,
        None,
        None,
        DeltaTimeSecond(60),
        None,
        StationType(5),
    );
    let etsi = DENM::new(
        ItsPduHeader::new(OrdinalNumber1B(2), MessageId(1), StationId(73)),
        DenmPayload::new(m, None, None, None),
    );
    let bytes = ItsMessage::DenmV2 {
        geonetworking: None,
        transport: None,
        etsi: Box::new(etsi),
    }
    .encode(EncodingRules::UPER)
    .unwrap();
    let out = decode_frame(&demo::wrap_uper(&bytes, 2002), 1).unwrap();
    assert_eq!(out.key, "denm:73:8");
    assert_eq!(out.kind, "DENM");
    assert!((out.latitude - 53.551086).abs() < 1e-7);
}
