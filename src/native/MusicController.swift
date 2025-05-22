import MediaPlayer
import Foundation

/// Apple Musicの一時停止機能
/// - Returns: 元々再生中だった場合はtrue、それ以外はfalse
@_cdecl("pause_apple_music_native")
public func pauseAppleMusicNative() -> Bool {
    // 権限チェック
    let authStatus = MPMediaLibrary.authorizationStatus()
    if authStatus != .authorized {
        // 権限要求（非同期）
        MPMediaLibrary.requestAuthorization { status in
            // この結果は現在の呼び出しでは使用されない
            // 次回の呼び出し時に権限が利用可能になることを期待
        }
        return false
    }
    
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    let wasPlaying = musicPlayer.playbackState == .playing
    
    if wasPlaying {
        musicPlayer.pause()
    }
    
    return wasPlaying
}

/// Apple Musicの再開機能
/// - Returns: 再開コマンドが成功した場合はtrue
@_cdecl("resume_apple_music_native")
public func resumeAppleMusicNative() -> Bool {
    // 権限チェック
    let authStatus = MPMediaLibrary.authorizationStatus()
    guard authStatus == .authorized else {
        return false
    }
    
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    musicPlayer.play()
    return true
}

/// Apple Musicの再生状態を取得
/// - Returns: 再生状態の数値（0: stopped, 1: playing, 2: paused, 3: interrupted, 4: seekingForward, 5: seekingBackward）
@_cdecl("get_music_playback_state")
public func getMusicPlaybackState() -> Int32 {
    // 権限チェック
    let authStatus = MPMediaLibrary.authorizationStatus()
    guard authStatus == .authorized else {
        return -1 // 権限なし
    }
    
    let musicPlayer = MPMusicPlayerController.applicationMusicPlayer
    return Int32(musicPlayer.playbackState.rawValue)
}

/// MediaPlayer権限の状態を取得
/// - Returns: 権限状態の数値（0: notDetermined, 1: denied, 2: restricted, 3: authorized）
@_cdecl("get_media_library_authorization_status")
public func getMediaLibraryAuthorizationStatus() -> Int32 {
    let authStatus = MPMediaLibrary.authorizationStatus()
    return Int32(authStatus.rawValue)
}

/// MediaPlayer権限を要求（非同期）
/// - Returns: 常にtrueを返す（権限要求が開始されたことを示す）
@_cdecl("request_media_library_authorization")
public func requestMediaLibraryAuthorization() -> Bool {
    MPMediaLibrary.requestAuthorization { status in
        // 権限要求結果の処理
        // 実際の結果は次回のget_media_library_authorization_statusで確認
    }
    return true
}