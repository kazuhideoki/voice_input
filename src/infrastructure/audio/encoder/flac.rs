use super::AudioEncodeError;
use flacenc::component::BitRepr;
use flacenc::error::Verify;

/// 16bit PCM (interleaved) から FLAC を生成してバイト列を返す
pub fn encode_flac_i16(
    samples: &[i16],
    sample_rate: u32,
    channels: u16,
) -> Result<Vec<u8>, AudioEncodeError> {
    // flacenc は i32 サンプルを想定するため変換
    let mut pcm_i32 = Vec::with_capacity(samples.len());
    pcm_i32.extend(samples.iter().copied().map(|s| s as i32));

    let cfg = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| AudioEncodeError::Flac(format!("config verify failed: {e:?}")))?;

    // 固定ブロックサイズ（cfg.block_size）でエンコード
    let source = flacenc::source::MemSource::from_samples(
        &pcm_i32,
        channels as usize,
        16,
        sample_rate as usize,
    );

    let mut sink = flacenc::bitsink::ByteSink::new();
    flacenc::encode_with_fixed_block_size(&cfg, source, cfg.block_size)
        .map_err(|e| AudioEncodeError::Flac(format!("encode failed: {e}")))?
        .write(&mut sink)
        .map_err(|e| AudioEncodeError::Flac(format!("write failed: {e}")))?;

    Ok(sink.as_slice().to_vec())
}
