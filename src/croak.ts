/** Short low croak: a filtered saw that falls in pitch. Best effort only. */
export function playCroak() {
  try {
    const ctx = new AudioContext();
    void ctx.resume();

    const now = ctx.currentTime;
    const osc = ctx.createOscillator();
    const filter = ctx.createBiquadFilter();
    const gain = ctx.createGain();

    osc.type = "sawtooth";
    osc.frequency.setValueAtTime(150, now);
    osc.frequency.exponentialRampToValueAtTime(92, now + 0.28);

    filter.type = "lowpass";
    filter.frequency.value = 750;

    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.exponentialRampToValueAtTime(0.06, now + 0.035);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.32);

    osc.connect(filter);
    filter.connect(gain);
    gain.connect(ctx.destination);
    osc.start(now);
    osc.stop(now + 0.35);
    osc.onended = () => {
      void ctx.close();
    };
  } catch {
    // Audio unavailable (autoplay policy, no output device): stay silent.
  }
}
