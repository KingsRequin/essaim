//! Capture de la sortie son de Windows (WASAPI loopback, même méthode que Prisme) et analyse
//! en continu. Les résultats sont publiés dans des atomiques lus par la boucle d'affichage.

use crate::dsp::Analyzer;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use wasapi::{DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

#[derive(Default)]
pub struct AudioShared {
    kicks: AtomicU32,
    bass: AtomicU32,
    pitch: AtomicU32,
    sensitivity: AtomicU32,
    min_gap: AtomicU32,
}

#[derive(Clone, Copy, Default)]
pub struct AudioView {
    pub kicks: u32,
    pub bass: f32,
    pub pitch: f32,
}

impl AudioShared {
    pub fn view(&self) -> AudioView {
        AudioView {
            kicks: self.kicks.load(Ordering::Relaxed),
            bass: f32::from_bits(self.bass.load(Ordering::Relaxed)),
            pitch: f32::from_bits(self.pitch.load(Ordering::Relaxed)),
        }
    }
    pub fn set_sensitivity(&self, s: f32) {
        self.sensitivity.store(s.to_bits(), Ordering::Relaxed);
    }
    pub fn set_min_gap(&self, s: f32) {
        self.min_gap.store(s.to_bits(), Ordering::Relaxed);
    }
    fn publish(&self, a: &Analyzer) {
        self.kicks.store(a.kicks, Ordering::Relaxed);
        self.bass.store(a.bass.to_bits(), Ordering::Relaxed);
        self.pitch.store(a.pitch.to_bits(), Ordering::Relaxed);
    }
}

pub struct AudioThread {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for AudioThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn start(shared: Arc<AudioShared>) -> std::io::Result<AudioThread> {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let thread = std::thread::Builder::new()
        .name("essaim-audio".into())
        .spawn(move || run(&shared, &s))?;
    Ok(AudioThread { stop, thread: Some(thread) })
}

fn run(shared: &AudioShared, stop: &AtomicBool) {
    if wasapi::initialize_mta().is_err() {
        log::warn!("audio : initialisation COM impossible");
    }
    let mut last_error_log = Instant::now() - Duration::from_secs(60);
    while !stop.load(Ordering::Relaxed) {
        if let Err(e) = stream(shared, stop) {
            if last_error_log.elapsed() > Duration::from_secs(30) {
                log::warn!("audio : capture interrompue : {e:#}, nouvel essai");
                last_error_log = Instant::now();
            }
            shared.bass.store(0f32.to_bits(), Ordering::Relaxed);
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    wasapi::deinitialize();
}

/// Une session de capture sur la sortie par défaut. `Ok` = arrêt demandé ; `Err` = à relancer.
fn stream(shared: &AudioShared, stop: &AtomicBool) -> anyhow::Result<()> {
    let enumerator = DeviceEnumerator::new()?;
    let device = enumerator.get_default_device(&Direction::Render)?;
    let device_id = device.get_id()?;
    let mut client = device.get_iaudioclient()?;
    let sample_rate = client.get_mixformat()?.get_samplespersec();
    let format = WaveFormat::new(32, 32, &SampleType::Float, sample_rate as usize, 2, None);
    let (default_period, _) = client.get_device_period()?;
    client.initialize_client(
        &format,
        &Direction::Capture,
        &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: default_period * 4 },
    )?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    let frames_max = client.get_buffer_size()? as usize;
    let mut bytes = vec![0u8; frames_max * 8];
    let mut mono = vec![0.0f32; frames_max];
    let mut an = Analyzer::new(sample_rate);
    client.start_stream()?;
    log::info!(
        "audio : loopback ouvert sur « {} » ({sample_rate} Hz)",
        device.get_friendlyname().unwrap_or_default()
    );

    let mut last_data = Instant::now();
    let mut device_checked = Instant::now();
    let result = loop {
        if stop.load(Ordering::Relaxed) {
            break Ok(());
        }
        an.sensitivity = f32::from_bits(shared.sensitivity.load(Ordering::Relaxed));
        let gap = f32::from_bits(shared.min_gap.load(Ordering::Relaxed));
        if gap > 0.0 {
            an.min_gap_s = gap as f64;
        }
        if event.wait_for_event(10).is_ok() {
            while capture.get_next_packet_size()?.unwrap_or(0) > 0 {
                let (frames, info) = capture.read_from_device(&mut bytes)?;
                let frames = frames as usize;
                if info.flags.silent {
                    an.push_silence(frames);
                } else {
                    for (m, fr) in mono.iter_mut().zip(bytes[..frames * 8].chunks_exact(8)) {
                        let l = f32::from_le_bytes([fr[0], fr[1], fr[2], fr[3]]);
                        let r = f32::from_le_bytes([fr[4], fr[5], fr[6], fr[7]]);
                        *m = 0.5 * (l + r);
                    }
                    an.push(&mono[..frames]);
                }
            }
            last_data = Instant::now();
        } else if last_data.elapsed() > Duration::from_millis(30) {
            // Rien n'est joué : Windows n'envoie aucun paquet, on injecte du silence pour que
            // le niveau de basse retombe.
            let n = (last_data.elapsed().as_secs_f64() * sample_rate as f64) as usize;
            an.push_silence(n.min(sample_rate as usize / 10));
            last_data = Instant::now();
        }
        shared.publish(&an);
        if device_checked.elapsed() >= Duration::from_secs(2) {
            device_checked = Instant::now();
            let current = enumerator.get_default_device(&Direction::Render).and_then(|d| d.get_id());
            if current.as_deref().ok() != Some(device_id.as_str()) {
                break Err(anyhow::anyhow!("sortie son par défaut changée"));
            }
        }
    };
    let _ = client.stop_stream();
    result
}
