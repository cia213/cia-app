use super::*;

#[test]
fn active_deadline_excludes_a_user_pause_and_bounds_running_work() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let registry = JobRegistry::default();
        let job = JobGuard::reserve(&registry, "active-deadline").unwrap();
        registry
            .jobs
            .lock()
            .unwrap()
            .get_mut("active-deadline")
            .unwrap()
            .paused = true;
        assert!(active_timeout(
            std::time::Duration::from_millis(50),
            Some(&job),
            tokio::time::sleep(std::time::Duration::from_millis(300))
        )
        .await
        .is_ok());
        registry
            .jobs
            .lock()
            .unwrap()
            .get_mut("active-deadline")
            .unwrap()
            .paused = false;
        assert!(active_timeout(
            std::time::Duration::from_millis(50),
            Some(&job),
            tokio::time::sleep(std::time::Duration::from_millis(500))
        )
        .await
        .is_err());
    });
}

#[cfg(target_os = "windows")]
#[test]
fn a_paused_job_starts_its_next_subprocess_suspended() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let registry = JobRegistry::default();
        let job = JobGuard::reserve(&registry, "paused-stage").unwrap();
        registry
            .jobs
            .lock()
            .unwrap()
            .get_mut("paused-stage")
            .unwrap()
            .paused = true;
        let mut command = Command::new("cmd.exe");
        command.args(["/D", "/C", "exit", "0"]);
        let mut child = job.spawn(&mut command).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            child.try_wait().unwrap().is_none(),
            "A subprocess must not execute while its job is paused"
        );
        {
            let mut jobs = registry.jobs.lock().unwrap();
            let state = jobs.get_mut("paused-stage").unwrap();
            state.owner.as_ref().unwrap().set_paused(false).unwrap();
            state.paused = false;
        }
        let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(
            status.success(),
            "One resume must release the initially suspended subprocess"
        );
    });
}

#[cfg(target_os = "windows")]
#[test]
fn pausing_one_process_owner_cannot_suspend_another_job() {
    let mut first = std::process::Command::new("powershell.exe");
    first.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Start-Sleep -Seconds 20",
    ]);
    let (mut first_child, first_owner) = process_control::spawn_std_owned(&mut first).unwrap();
    first_owner.set_paused(true).unwrap();
    let mut second = std::process::Command::new("cmd.exe");
    second.args(["/D", "/C", "exit", "0"]);
    let (mut second_child, second_owner) = process_control::spawn_std_owned(&mut second).unwrap();
    let started = std::time::Instant::now();
    let second_status = loop {
        if let Some(status) = second_child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "An unrelated job must remain running while the first is paused"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(second_status.success());
    assert!(first_child.try_wait().unwrap().is_none());
    first_owner.terminate();
    first_child.wait().unwrap();
    drop(second_owner);
}

fn two_track_info() -> VideoInfo {
    parse_video_info(&serde_json::json!({
        "streams": [
            {"index":0,"codec_type":"video","width":160,"height":96,
             "avg_frame_rate":"25/1","r_frame_rate":"25/1","duration":"2.0",
             "nb_frames":"50","sample_aspect_ratio":"4:3","start_time":"0.0"},
            {"index":1,"codec_type":"audio","codec_name":"aac","channels":1,
             "channel_layout":"mono","duration":"2.0","start_time":"0.0",
             "tags":{"language":"eng","title":"Game"},"disposition":{"default":1}},
            {"index":2,"codec_type":"audio","codec_name":"aac","channels":1,
             "channel_layout":"mono","duration":"2.0","start_time":"0.0",
             "tags":{"language":"fra","title":"Voice"},"disposition":{"default":0}}
        ]
    }))
    .unwrap()
}

#[test]
fn utf8_diagnostics_survive_every_byte_boundary_and_byte_at_a_time() {
    let expected = "C:\\é space\\视频\\🎞 input.mp4: interpolation complete\r\n";
    let bytes = expected.as_bytes();
    for split in 0..=bytes.len() {
        let mut tail = Vec::new();
        let mut decoded = decode_log_chunk(&mut tail, &bytes[..split]);
        decoded.push_str(&decode_log_chunk(&mut tail, &bytes[split..]));
        assert_eq!(decoded, expected, "UTF-8 split at byte {split}");
        assert!(tail.is_empty());
    }
    let mut tail = Vec::new();
    let mut decoded = String::new();
    for byte in bytes {
        decoded.push_str(&decode_log_chunk(&mut tail, &[*byte]));
        assert!(
            tail.len() <= 3,
            "Only an incomplete UTF-8 sequence is retained"
        );
    }
    assert_eq!(decoded, expected);
    assert!(tail.is_empty());
}

#[test]
fn invalid_utf8_does_not_hide_following_diagnostic_and_eof_tail_is_preserved() {
    let mut tail = Vec::new();
    assert_eq!(
        decode_log_chunk(&mut tail, b"before\xff after"),
        "before� after"
    );
    assert!(tail.is_empty());
    assert_eq!(decode_log_chunk(&mut tail, &[0xf0, 0x9f]), "");
    assert_eq!(tail, [0xf0, 0x9f]);
    let eof = String::from_utf8_lossy(&tail);
    assert_eq!(
        eof, "�",
        "The caller can report an incomplete final sequence at EOF"
    );
}

#[test]
fn persisted_preferences_require_typed_values_and_execution_ranges() {
    let rife = serde_json::json!({"mode":"boost","factor":3,"crf":18,"preset":"medium","precision":"fp32","encoder":"libx264"});
    let render = serde_json::json!({"fps":25,"speed":1.07,"blendIntensity":1.0,"brightness":1.0,"saturation":1.0,"contrast":1.0,"lutEnabled":"no","lutOpacity":1.0,"borderless":"no","encoder":"libx264"});
    assert!(validate_ui_settings(&rife, &render).is_ok());
    for invalid in [
        serde_json::json!([]),
        serde_json::json!(null),
        serde_json::json!("settings"),
    ] {
        assert!(validate_ui_settings(&invalid, &render).is_err());
        assert!(validate_ui_settings(&rife, &invalid).is_err());
    }
    for (key, value) in [
        ("fps", serde_json::json!("25")),
        ("fps", serde_json::json!(25.5)),
        ("speed", serde_json::json!(0.0)),
        ("speed", serde_json::json!(true)),
        ("speed", serde_json::json!(null)),
        ("lutEnabled", serde_json::json!(true)),
        ("encoder", serde_json::json!("unexpected-encoder")),
    ] {
        let mut invalid = render.clone();
        invalid[key] = value;
        assert!(
            validate_ui_settings(&rife, &invalid).is_err(),
            "Invalid {key}"
        );
    }
    let mut invalid = rife.clone();
    invalid["factor"] = serde_json::json!(2.5);
    assert!(validate_ui_settings(&invalid, &render).is_err());
}

#[test]
fn tail_recovery_accepts_reported_nvidia_case_but_rejects_major_loss() {
    assert_eq!(validate_tail_recovery(1621, 1613, 25).unwrap(), 8);
    assert_eq!(validate_tail_recovery(50, 49, 25).unwrap(), 1);
    assert_eq!(validate_tail_recovery(50, 50, 25).unwrap(), 0);
    assert!(validate_tail_recovery(50, 1, 25).is_err());
    assert!(validate_tail_recovery(1621, 0, 25).is_err());
    assert!(
        validate_tail_recovery(1621, 1595, 25).is_err(),
        "More than one second must not be silently held"
    );
}

#[test]
fn matroska_video_duration_wins_over_longer_audio_and_container() {
    let info = parse_video_info(&serde_json::json!({
        "format":{"duration":"3.0"},
        "streams":[
            {"index":0,"codec_type":"video","width":160,"height":96,
             "avg_frame_rate":"24/1","r_frame_rate":"24/1","start_time":"0.0",
             "tags":{"DURATION":"00:00:00.500000000"}},
            {"index":1,"codec_type":"audio","codec_name":"aac","channels":1,"duration":"3.0"}
        ]
    }))
    .unwrap();
    assert_eq!(info.duration, 0.5);
    assert_eq!(info.frame_count, None);
    assert_eq!(info.audio_tracks.len(), 1);
    assert_eq!(info.audio_tracks[0].duration, Some(3.0));
    assert_eq!(parse_clock_seconds("01:02:03.125"), Some(3723.125));
    for value in ["not-a-clock", "00:00", "NaN:00:00", "00:00:inf"] {
        assert_eq!(parse_clock_seconds(value), None, "{value}");
    }
}

#[test]
fn output_contract_rejects_audio_loss_changed_channels_metadata_and_offsets() {
    let source = two_track_info();
    assert!(validate_output_info(&source, &source, 2.0, 25.0, Some(50)).is_ok());
    let mut invalid = source.clone();
    invalid.audio_tracks.pop();
    assert!(validate_output_info(&invalid, &source, 2.0, 25.0, Some(50))
        .unwrap_err()
        .contains("number of audio tracks"));
    let mut invalid = source.clone();
    invalid.audio_tracks[0].channels = Some(2);
    assert!(validate_output_info(&invalid, &source, 2.0, 25.0, Some(50))
        .unwrap_err()
        .contains("channel"));
    let mut invalid = source.clone();
    invalid.audio_tracks[0].language = None;
    assert!(validate_output_info(&invalid, &source, 2.0, 25.0, Some(50))
        .unwrap_err()
        .contains("language"));
    let mut invalid = source.clone();
    invalid.audio_tracks[1].title = None;
    assert!(validate_output_info(&invalid, &source, 2.0, 25.0, Some(50))
        .unwrap_err()
        .contains("title"));
    for (start, duration) in [(0.3, 2.0), (0.0, 1.0)] {
        let mut invalid = source.clone();
        invalid.audio_tracks[1].start_time = start;
        invalid.audio_tracks[1].duration = Some(duration);
        assert!(validate_output_info(&invalid, &source, 2.0, 25.0, Some(50))
            .unwrap_err()
            .contains("timeline"));
    }
}

#[test]
fn output_audio_default_is_canonical_when_source_has_no_default_track() {
    let mut source = two_track_info();
    source.audio_tracks[0].default = false;
    source.audio_tracks[1].forced = true;
    let mut output = source.clone();
    output.audio_tracks[0].default = true;
    assert!(validate_output_info(&output, &source, 2.0, 25.0, Some(50)).is_ok());
    output.audio_tracks[0].default = false;
    assert!(validate_output_info(&output, &source, 2.0, 25.0, Some(50)).is_err());
    output.audio_tracks[0].default = true;
    output.audio_tracks[1].forced = false;
    assert!(validate_output_info(&output, &source, 2.0, 25.0, Some(50)).is_err());
}

#[test]
fn audio_finish_uses_rubberband_pitch_lock_and_normalizes_relative_offsets() {
    let mut source = two_track_info();
    source.video_start_time = 0.25;
    let mut early = source.audio_tracks[0].clone();
    early.start_time = 0.0;
    let early_filter = audio_finish_filter(&source, &early, 1.07, 1.88);
    assert!(early_filter.contains("atrim=start=0.250000000"));
    assert!(early_filter.contains("rubberband=tempo=1.07:pitch=1.0"));
    let mut late = source.audio_tracks[1].clone();
    late.start_time = 0.75;
    let late_filter = audio_finish_filter(&source, &late, 1.07, 1.88);
    assert!(late_filter.contains("adelay=500.000000:all=1"));
    let neutral = audio_finish_filter(&source, &late, 1.0, 2.0);
    assert!(!neutral.contains("rubberband"));
    assert!(neutral.contains("atrim=duration=2.000000000"));
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "reliability-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("Failed media fixtures retained at {}", self.0.display());
            return;
        }
        let expected = Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
        assert!(
            self.0.starts_with(expected)
                && self
                    .0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("reliability-")
        );
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn media_fixture_paths() -> (PathBuf, PathBuf, PathBuf) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/runtime");
    let ffmpeg = base.join("ffmpeg/ffmpeg.exe");
    let ffprobe = base.join("ffmpeg/ffprobe.exe");
    let smoothie = base.join("smoothie");
    assert!(
        ffmpeg.is_file() && ffprobe.is_file(),
        "Prepare the local bundled runtime before this ignored test"
    );
    (ffmpeg, ffprobe, smoothie)
}

async fn run_fixture(command: &mut Command) -> std::process::Output {
    let output = command.output().await.unwrap();
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
#[ignore = "requires the local bundled FFmpeg runtime"]
fn decoded_frame_fallback_rejects_one_missing_frame_even_with_tolerable_duration() {
    let (ffmpeg, ffprobe, _) = media_fixture_paths();
    let scratch = Scratch::new("frame-count");
    let output = scratch.0.join("frames.mkv");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        run_fixture(
            Command::new(&ffmpeg)
                .args([
                    "-hide_banner",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=160x96:rate=25:duration=2",
                    "-an",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-vf",
                    "setsar=4/3",
                    "-n",
                ])
                .arg(&output),
        )
        .await;
        let source = probe_video(&output.to_string_lossy(), &ffprobe)
            .await
            .unwrap();
        assert!(
            source.frame_count.is_none(),
            "Matroska must exercise decoded frame verification"
        );
        let registry = JobRegistry::default();
        let job = JobGuard::reserve(&registry, "decoded-count").unwrap();
        validate_output(&output, &ffprobe, &source, 2.0, 25.0, Some(50), &job)
            .await
            .unwrap();
        let error = validate_output(&output, &ffprobe, &source, 2.0, 25.0, Some(49), &job)
            .await
            .unwrap_err();
        assert!(error.contains("decoded output frame count"), "{error}");
    });
}

#[test]
#[ignore = "requires the local bundled FFmpeg and Smoothie runtime"]
fn native_audio_finishing_preserves_both_tracks_pitch_offsets_and_video_geometry() {
    let (ffmpeg, ffprobe, smoothie_root) = media_fixture_paths();
    let smoothie = smoothie_root.join("bin/smoothie-rs.exe");
    assert!(smoothie.is_file());
    let scratch = Scratch::new("audio-finish");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        for (case, video_offset, first_offset, second_offset, speed) in [
            ("neutral",0.0,0.0,0.0,1.0),
            ("sped",0.0,0.0,0.0,1.07),
            ("late-audio",0.0,0.2,0.55,1.07),
            ("early-audio",0.2,0.0,0.55,1.0),
            ("no-source-default",0.0,0.0,0.0,1.07),
        ] {
            let directory = scratch.0.join(case);
            fs::create_dir(&directory).unwrap();
            let input = directory.join("input.mkv");
            let filter = format!("[0:v]setsar=4/3,setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709,setpts=PTS+{video_offset}/TB[v];[1:a]asetpts=PTS+{first_offset}/TB[a0];[2:a]asetpts=PTS+{second_offset}/TB[a1]");
            let dispositions = if case == "no-source-default" { ("0", "forced") } else { ("default", "0") };
            run_fixture(Command::new(&ffmpeg).args(["-hide_banner","-v","error","-f","lavfi","-i","testsrc2=size=160x96:rate=120:duration=2","-f","lavfi","-i","sine=frequency=400:sample_rate=48000:duration=2.5","-f","lavfi","-i","sine=frequency=800:sample_rate=48000:duration=2.5","-filter_complex"]).arg(filter).args(["-map","[v]","-map","[a0]","-map","[a1]","-c:v","libx264","-preset","ultrafast","-crf","18","-c:a","aac","-b:a","160k","-color_primaries","bt709","-color_trc","bt709","-colorspace","bt709","-metadata:s:a:0","language=eng","-metadata:s:a:0","title=Game","-metadata:s:a:1","language=fra","-metadata:s:a:1","title=Voice","-disposition:a:0",dispositions.0,"-disposition:a:1",dispositions.1,"-n"]).arg(&input)).await;
            let original = fs::read(&input).unwrap();
            let info = probe_video(&input.to_string_lossy(), &ffprobe).await.unwrap();
            assert_eq!(info.audio_tracks.len(), 2);
            assert!((info.duration - 2.0).abs() < 0.03, "{case}: video duration={} must ignore longer audio",info.duration);
            assert_eq!(info.audio_tracks[0].title.as_deref(), Some("Game"));
            assert_eq!(info.audio_tracks[1].title.as_deref(), Some("Voice"));
            assert_eq!(info.color_space.as_deref(), Some("bt709"));
            assert_eq!(info.color_transfer.as_deref(), Some("bt709"));
            assert_eq!(info.color_primaries.as_deref(), Some("bt709"));
            let planned = smoothie_output_frames(info.duration, 25, speed).unwrap();
            let duration = planned as f64 / 25.0;
            let video = directory.join("output.mp4");
            let mut overrides = smoothie_overrides(vec!["lut;enabled;no".into(),"frame blending;intensity;1.0".into()],25,"libx264",None,&info.sample_aspect_ratio,speed,info.duration).unwrap();
            preserve_smoothie_color_metadata(&mut overrides,&info);
            let encoding = overrides.iter_mut().find(|v|v.starts_with("output;enc args;")).unwrap();
            encoding.push_str(" -an");
            let scoped_path = env::join_paths(std::iter::once(ffmpeg.parent().unwrap().to_path_buf()).chain(env::split_paths(&env::var_os("PATH").unwrap_or_default()))).unwrap();
            let registry = JobRegistry::default();
            let job = JobGuard::reserve(&registry, case).unwrap();
            let mut engine = Command::new(&smoothie);
            engine.current_dir(&smoothie_root).env("PATH",scoped_path).arg("-i").arg(&input).arg("-o").arg(&video).arg("--recipe").arg(smoothie_root.join("recipe.ini")).arg("--override").args(&overrides).stdout(Stdio::piped()).stderr(Stdio::piped());
            let result = job.spawn(&mut engine).unwrap().wait_with_output().await.unwrap();
            assert!(result.status.success(),"{case}: {}",String::from_utf8_lossy(&result.stderr));
            let log = String::from_utf8_lossy(&result.stderr);
            let real = log.lines().filter(|line|line.contains("showinfo@cia_real")).filter_map(|line|line.split(" n:").nth(1).and_then(|rest|rest.split_whitespace().next()).and_then(|n|n.parse::<u64>().ok())).max().map(|n|n+1).expect("The actual engine must expose its pre-padding frame count");
            validate_tail_recovery(planned,real,25).unwrap();
            let finished = finish_smoothie_audio(&input.to_string_lossy(),&video,&ffmpeg,&info,speed,planned,25,&job).await.unwrap();
            validate_output(&finished,&ffprobe,&info,duration,25.0,Some(planned),&job).await.unwrap();
            let output_info = probe_video(&finished.to_string_lossy(),&ffprobe).await.unwrap();
            assert_eq!((output_info.width,output_info.height),(160,96));
            assert_eq!(output_info.sample_aspect_ratio,"4:3");
            assert_eq!(output_info.color_space.as_deref(),Some("bt709"));
            assert_eq!(output_info.color_transfer.as_deref(),Some("bt709"));
            assert_eq!(output_info.color_primaries.as_deref(),Some("bt709"));
            assert!(output_info.audio_tracks[0].default);
            assert!(!output_info.audio_tracks[1].default);
            assert_eq!(output_info.audio_tracks[1].forced, case == "no-source-default");
            for (track,expected_pitch) in [(0,400.0),(1,800.0)] {
                let decoded = run_fixture(Command::new(&ffmpeg).args(["-hide_banner","-v","error","-i"]).arg(&finished).args(["-map"]).arg(format!("0:a:{track}")).args(["-ss","0.8","-t","0.6","-ac","1","-ar","48000","-f","f32le","pipe:1"])).await;
                let samples:Vec<f32>=decoded.stdout.as_chunks::<4>().0.iter().map(|bytes|f32::from_le_bytes(*bytes)).collect();
                assert!(samples.len() > 20_000, "{case}: track {track} must contain a measurable audio segment");
                let crossings = samples.windows(2).filter(|pair|pair[0]<=0.0 && pair[1]>0.0).count();
                let pitch = crossings as f64 * 48000.0 / (samples.len()-1) as f64;
                assert!((pitch-expected_pitch).abs()<4.0,"{case}: track{track} pitch{pitch} instead of{expected_pitch}");
                println!("{case}: track {track} pitch {pitch:.2} Hz (source {expected_pitch:.0} Hz)");
                let delay=(info.audio_tracks[track].start_time-info.video_start_time).max(0.0)/speed;
                if delay>0.15 {
                    let early=run_fixture(Command::new(&ffmpeg).args(["-hide_banner","-v","error","-i"]).arg(&finished).arg("-map").arg(format!("0:a:{track}")).args(["-ss","0.02","-t","0.08","-ac","1","-ar","48000","-f","f32le","pipe:1"])).await;
                    let samples:Vec<f32>=early.stdout.as_chunks::<4>().0.iter().map(|bytes|f32::from_le_bytes(*bytes)).collect();
                    assert!(!samples.is_empty(), "{case}: the leading audio interval must decode");
                    let rms=(samples.iter().map(|v|f64::from(*v).powi(2)).sum::<f64>()/samples.len() as f64).sqrt();
                    assert!(rms<0.002,"{case}: late track{track} lost its leading silence ({rms})");
                }
            }
            assert_eq!(fs::read(&input).unwrap(),original,"Source bytes must remain unchanged");
            println!("{case}: two AAC tracks, original pitch, bounded offsets, {planned} frames at 25 FPS, SAR 4:3 and BT709");
        }
    });
}
