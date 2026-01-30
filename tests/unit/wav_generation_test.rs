use voice_input::infrastructure::audio::cpal_backend::CpalAudioBackend;

/// WAVヘッダーがRIFF/format/data構造を満たす
#[test]
fn wav_header_has_expected_structure() {
    // 1秒のステレオ16bit 48kHzオーディオ
    let data_len = 48000 * 2 * 2; // sample_rate * channels * bytes_per_sample
    let header = CpalAudioBackend::create_wav_header(data_len, 48000, 2, 16);
    
    // ヘッダーサイズは44バイト
    assert_eq!(header.len(), 44);
    
    // RIFFチャンクの検証
    assert_eq!(&header[0..4], b"RIFF");
    let file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    assert_eq!(file_size, 36 + data_len);
    assert_eq!(&header[8..12], b"WAVE");
    
    // fmtチャンクの検証
    assert_eq!(&header[12..16], b"fmt ");
    let fmt_size = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
    assert_eq!(fmt_size, 16);
    let format = u16::from_le_bytes([header[20], header[21]]);
    assert_eq!(format, 1); // PCMフォーマット
    let channels = u16::from_le_bytes([header[22], header[23]]);
    assert_eq!(channels, 2);
    let sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
    assert_eq!(sample_rate, 48000);
    let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
    assert_eq!(byte_rate, 48000 * 2 * 2); // 192000
    let block_align = u16::from_le_bytes([header[32], header[33]]);
    assert_eq!(block_align, 4); // 2 channels * 16 bits / 8
    let bits_per_sample = u16::from_le_bytes([header[34], header[35]]);
    assert_eq!(bits_per_sample, 16);
    
    // dataチャンクの検証
    assert_eq!(&header[36..40], b"data");
    let data_size = u32::from_le_bytes([header[40], header[41], header[42], header[43]]);
    assert_eq!(data_size, data_len);
}

/// モノラル設定のWAVヘッダーが正しい
#[test]
fn wav_header_supports_mono() {
    // モノラル設定でのヘッダー生成
    let data_len = 44100 * 1 * 2; // 44.1kHz, mono, 16bit
    let header = CpalAudioBackend::create_wav_header(data_len, 44100, 1, 16);
    
    assert_eq!(header.len(), 44);
    
    // チャンネル数確認
    let channels = u16::from_le_bytes([header[22], header[23]]);
    assert_eq!(channels, 1);
    
    // バイトレート確認
    let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
    assert_eq!(byte_rate, 44100 * 1 * 2); // 88200
    
    // ブロックアライン確認
    let block_align = u16::from_le_bytes([header[32], header[33]]);
    assert_eq!(block_align, 2); // 1 channel * 16 bits / 8
}

/// サンプルレートがヘッダーに正しく反映される
#[test]
fn wav_header_reflects_sample_rate() {
    let sample_rates = vec![8000, 16000, 22050, 44100, 48000, 96000];
    
    for rate in sample_rates {
        let data_len = rate * 2 * 2; // 1秒分のステレオ16bit
        let header = CpalAudioBackend::create_wav_header(data_len, rate, 2, 16);
        
        let header_sample_rate = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        assert_eq!(header_sample_rate, rate);
        
        let byte_rate = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);
        assert_eq!(byte_rate, rate * 2 * 2);
    }
}

/// データ長0でもWAVヘッダーを生成できる
#[test]
fn wav_header_allows_empty_data() {
    // データ長0でのヘッダー生成
    let header = CpalAudioBackend::create_wav_header(0, 48000, 2, 16);
    
    assert_eq!(header.len(), 44);
    
    // ファイルサイズは36（ヘッダー44 - 8）
    let file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    assert_eq!(file_size, 36);
    
    // データサイズは0
    let data_size = u32::from_le_bytes([header[40], header[41], header[42], header[43]]);
    assert_eq!(data_size, 0);
}
