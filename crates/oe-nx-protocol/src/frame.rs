//! One bounded frame per request/response; read exactly declared lengths and
//! never wait for stdin EOF. Generic Read/Write only: no process or device I/O
//! is selected/opened here. Trailing frames remain unread for the caller.
use crate::*;
use anyhow::{Result, ensure};
use serde_json::Value;
use std::io::{Read, Write};

pub const FRAME_MAGIC: [u8; 8] = *b"OENXv2\0\0";
pub const FRAME_HEADER_BYTES: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub record: Value,
    pub payload: Vec<u8>,
}
impl Frame {
    /// Responses carrying a fetched ZIP must declare/bind that ZIP in their
    /// typed receipt. The caller passes its validated identity here. Empty
    /// response payloads require None; an unexpected payload is not discarded.
    pub fn validate_payload(&self, expected: Option<&BlobIdentity>) -> Result<()> {
        match expected {
            Some(expected) => {
                ensure!(
                    expected.bytes > 0 && expected.bytes <= MAX_PAYLOAD_BYTES,
                    "invalid response payload bound"
                );
                expected.verify(&self.payload)
            }
            None => {
                ensure!(self.payload.is_empty(), "unexpected response payload");
                Ok(())
            }
        }
    }
}

fn read_record(reader: &mut impl Read) -> Result<(Value, u64)> {
    let mut header = [0u8; FRAME_HEADER_BYTES];
    reader.read_exact(&mut header)?;
    ensure!(header[..8] == FRAME_MAGIC, "unknown NX frame magic/version");
    let json_len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    let payload_len = u64::from_le_bytes(header[12..20].try_into().unwrap());
    // Bounds precede both data allocations; malformed headers are not drained.
    ensure!(
        (1..=MAX_RECORD_BYTES).contains(&json_len),
        "frame JSON length exceeds bound"
    );
    ensure!(
        payload_len <= MAX_PAYLOAD_BYTES,
        "frame payload length exceeds bound"
    );
    let mut bytes = vec![0u8; json_len];
    reader.read_exact(&mut bytes)?;
    let record = json::strict_json(&bytes)?;
    ensure!(record.is_object(), "frame record must be an object");
    Ok((record, payload_len))
}
fn read_payload(reader: &mut impl Read, length: u64) -> Result<Vec<u8>> {
    let mut bytes = vec![0u8; usize::try_from(length)?];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub fn read_frame(reader: &mut impl Read) -> Result<Frame> {
    let (record, length) = read_record(reader)?;
    Ok(Frame {
        record,
        payload: read_payload(reader, length)?,
    })
}
pub fn write_frame(writer: &mut impl Write, frame: &Frame) -> Result<()> {
    ensure!(
        frame.record.is_object() && frame.payload.len() as u64 <= MAX_PAYLOAD_BYTES,
        "invalid frame record or payload bound"
    );
    let record = serde_json::to_vec(&frame.record)?;
    ensure!(
        !record.is_empty() && record.len() <= MAX_RECORD_BYTES,
        "frame JSON length exceeds bound"
    );
    writer.write_all(&FRAME_MAGIC)?;
    writer.write_all(&(record.len() as u32).to_le_bytes())?;
    writer.write_all(&(frame.payload.len() as u64).to_le_bytes())?;
    writer.write_all(&record)?;
    writer.write_all(&frame.payload)?;
    // Do not close or read the stream. Transport owns any flush policy.
    Ok(())
}
fn validate_request_length(request: &Request, length: u64) -> Result<()> {
    request.validate()?;
    match request {
        Request::Stage { archive, .. }
        | Request::TransferPut {
            payload: archive, ..
        }
        | Request::TransferPutBatch {
            payload: archive, ..
        } => ensure!(
            archive.bytes == length,
            "stage payload length differs from archive identity"
        ),
        _ => ensure!(
            length == 0,
            "only stage or transfer payload requests may carry payload"
        ),
    }
    Ok(())
}
pub fn read_request(reader: &mut impl Read) -> Result<(Request, Vec<u8>)> {
    let (record, length) = read_record(reader)?;
    let request: Request = serde_json::from_value(record)?;
    validate_request_length(&request, length)?; // before payload allocation/read
    let payload = read_payload(reader, length)?;
    if let Request::Stage { archive, .. }
    | Request::TransferPut {
        payload: archive, ..
    }
    | Request::TransferPutBatch {
        payload: archive, ..
    } = &request
    {
        archive.verify(&payload)?;
    }
    Ok((request, payload))
}
pub fn write_request(writer: &mut impl Write, request: &Request, payload: &[u8]) -> Result<()> {
    validate_request_length(request, payload.len() as u64)?;
    if let Request::Stage { archive, .. }
    | Request::TransferPut {
        payload: archive, ..
    }
    | Request::TransferPutBatch {
        payload: archive, ..
    } = request
    {
        archive.verify(payload)?;
    }
    write_frame(
        writer,
        &Frame {
            record: serde_json::to_value(request)?,
            payload: payload.to_vec(),
        },
    )
}
