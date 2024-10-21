use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use std::fmt::Debug;
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};

pub trait ToSample: Sample + Debug {
    fn to_f32(self) -> f32;
    fn from_f32(sample: f32) -> Self;
}

impl ToSample for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }

    fn from_f32(sample: f32) -> Self {
        (sample * i16::MAX as f32) as i16
    }
}

impl ToSample for f32 {
    fn to_f32(self) -> f32 {
        self
    }

    fn from_f32(sample: f32) -> Self {
        sample
    }
}

fn main() -> Result<(), anyhow::Error> {
    let host = cpal::default_host();

    // -= READ FILE, COPY TO VECTOR, PLAY BACK =-
    let wav_file_path = "./piano.wav";
    let mut reader = hound::WavReader::open(wav_file_path)?;
    let spec = reader.spec();
    println!("Sample rate: {}", spec.sample_rate);
    println!("Channels: {}", spec.channels);
    println!("Bits per sample: {}", spec.bits_per_sample);

    let mut samples: Vec<f32> = Vec::new();

    for sample in reader.samples::<i16>() {
        match sample {
            Ok(s) => samples.push(s as f32 / i16::MAX as f32),
            Err(e) => eprintln!("Error reading sample: {}", e),
        }
    }

    let delayed_signal = delay(samples, 44100, 500.0, 0.5);

    let device = host
        .default_output_device()
        .expect("Failed to find default output device");
    let config = device.default_output_config()?;

    let sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    // wrap samples in Arc<Mutex<Vec<f32>>> to safely share between threads
    let sample_data = Arc::new(Mutex::new(delayed_signal));
    let mut sample_index = 0;

    match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let stream = device.build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let samples = sample_data.lock().unwrap();
                    for frame in data.chunks_mut(channels) {
                        for sample in frame.iter_mut() {
                            if sample_index < samples.len() {
                                *sample = samples[sample_index].into();
                                sample_index += 1;
                            } else {
                                *sample = 0.0;
                            }
                        }
                    }
                },
                move |err| {
                    eprintln!("Error during playback: {}", err);
                },
                None,
            )?;

            stream.play()?;

            let file_duration = reader.duration() / sample_rate;
            std::thread::sleep(std::time::Duration::from_secs(file_duration.into()));
        }
        _ => return Err(anyhow::anyhow!("Unsupported sample format")),
    }
    Ok(())
}

fn delay<T>(input: Vec<T>, sample_rate: u32, delay_time_in_ms: f32, feedback_gain: f32) -> Vec<T>
where
    T: ToSample,
{
    let delay_samples = (sample_rate as f32 * delay_time_in_ms / 1000.0) as usize;

    let samples: Vec<f32> = input.iter().map(|s| s.to_f32()).collect();

    println!("Total samples: {}", samples.len());

    let mut new_samples = samples.clone();

    for (index, sample) in samples.iter().enumerate() {
        if index <= delay_samples {
            new_samples[index] = *sample;
        } else {
            new_samples[index] = *sample + (feedback_gain * samples[index - delay_samples]);
        }
    }

    new_samples.into_iter().map(T::from_f32).collect()
}
