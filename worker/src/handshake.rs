use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::fetcher;

const HANDSHAKE_OK: u8 = 0;
const HANDSHAKE_MALRFOMED: u8 = 1;
const HANDSHAKE_NOT_FOUND: u8 = 2;

pub struct HandshakeSuccess {
    /// Function uuid to run
    pub function_uuid: String,
    /// .wasm module to run in bytes
    pub wasm_bytes: Vec<u8>,
}

pub async fn handle_handshake<R>(
    stream: R,
    function_fetcher: fetcher::FunctionFetcher,
) -> Result<HandshakeSuccess, ()>
where
    R: AsyncReadExt + AsyncWriteExt + Unpin + Send,
{
    let mut stream = stream;

    // version is 8bit integer
    let version = match stream.read_u8().await {
        Ok(v) => v,
        Err(e) => {
            log::error!("Unable to read version field for handshake: {e}");
            stream.write_u8(HANDSHAKE_MALRFOMED).await.unwrap();
            return Err(());
        }
    };

    match version {
        1 => {}
        _ => return Err(()),
    };

    let function_uuid_bytes = &mut [0_u8; 16];
    let function_uuid = match stream.read_exact(function_uuid_bytes).await {
        Ok(_) => uuid_from_be_bytes(function_uuid_bytes),
        Err(e) => {
            log::error!("Unable to read function uuid field for handshake: {e}");
            stream.write_u8(HANDSHAKE_MALRFOMED).await.unwrap();
            return Err(());
        }
    };

    // UNIX timestamp (seconds) for the last deployment of this function
    let last_deployment = match stream.read_u64().await {
        Ok(len) => len,
        Err(e) => {
            log::error!("Unable to read last deployment field for handshake: {e}");
            stream.write_u8(HANDSHAKE_MALRFOMED).await.unwrap();
            return Err(());
        }
    };

    log::debug!("start:function_fetcher.fetch");
    let wasm_bytes = match function_fetcher
        .fetch(&function_uuid, last_deployment)
        .await
    {
        Ok(bytes) => bytes,
        Err(e) => {
            // TODO: send error back
            // Probably worth implementing a default handler for this kind of cases
            stream.write_u8(HANDSHAKE_NOT_FOUND).await.unwrap();
            log::error!("Could not fetch function \"{function_uuid}\": {e:?}");
            return Err(());
        }
    };
    log::debug!("finish:function_fetcher.fetch handshake OK");

    // Handshake OK
    stream.write_u8(HANDSHAKE_OK).await.unwrap();

    return Ok(HandshakeSuccess {
        function_uuid,
        wasm_bytes,
    });
}

fn uuid_from_be_bytes(bytes: &mut [u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}
