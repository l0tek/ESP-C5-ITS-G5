//! Synthetic packet fixtures produced with the same ASN.1 encoder as the parser.
use c_its_parser::standards::{cam_1_4_1::cam_pdu_descriptions::*, cdd_1_3_1_1::its_container::*};
use c_its_parser::{EncodingRules, ItsMessage};

pub fn cam_frame(station: u32, lat: i32, lon: i32) -> Vec<u8> {
    let position = ReferencePosition::new(
        Latitude(lat),
        Longitude(lon),
        PosConfidenceEllipse::new(
            SemiAxisLength(4095),
            SemiAxisLength(4095),
            HeadingValue(3601),
        ),
        Altitude::new(AltitudeValue(800001), AltitudeConfidence::unavailable),
    );
    let params = CamParameters::new(
        BasicContainer::new(StationType(15), position),
        HighFrequencyContainer::rsuContainerHighFrequency(RSUContainerHighFrequency::new(None)),
        None,
        None,
    );
    let cam = CAM::new(
        ItsPduHeader::new(2, 2, StationID(station)),
        CoopAwareness::new(GenerationDeltaTime(1), params),
    );
    let encoded = ItsMessage::Cam {
        geonetworking: None,
        transport: None,
        etsi: Box::new(cam),
    }
    .encode(EncodingRules::UPER)
    .expect("synthetic CAM must encode");
    wrap_uper(&encoded, 2001)
}
pub fn wrap_uper(payload: &[u8], port: u16) -> Vec<u8> {
    let mut frame = vec![0u8; 24];
    frame[0] = 8;
    frame[4..10].fill(0xff);
    frame[10..16].copy_from_slice(&[2, 0, 0, 0, 0, 1]);
    frame[16..22].fill(0xff);
    frame.extend_from_slice(&[0xaa, 0xaa, 3, 0, 0, 0, 0x89, 0x47]);
    // GeoNetworking Basic, Common and SHB extended header, then BTP-B.
    frame.extend_from_slice(&[0x11, 0, 0x05, 1]);
    frame.extend_from_slice(&[0x20, 0x50, 0, 0]);
    frame.extend_from_slice(&((payload.len() + 4) as u16).to_be_bytes());
    frame.extend_from_slice(&[1, 0]);
    frame.extend_from_slice(&[0u8; 28]);
    frame.extend_from_slice(&port.to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(payload);
    frame
}
pub fn frames() -> Vec<Vec<u8>> {
    vec![
        cam_frame(1001, 535_510_860, 99_937_000),
        cam_frame(1002, 535_520_500, 99_960_000),
        cam_frame(1003, 535_500_000, 99_910_000),
    ]
}
