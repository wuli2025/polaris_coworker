/**
 * 手机端麦克风录音 —— 语音输入的采集半边。
 *
 * **为什么不用 MediaRecorder**:安卓 WebView(Chromium 内核)的 MediaRecorder 默认且
 * 只产 `audio/webm;codecs=opus`,而火山两条识别链路(openspeech 极速版 / 方舟音频理解)
 * **都不收 webm** —— openspeech 只认 wav/mp3/ogg-opus,方舟只认 mp3/wav/aac/m4a/pcm 等。
 * 所以这里照桌面 web 版(src/lib/webVoice.ts)的做法:抓 Float32 PCM,自己重采样到 16kHz
 * 单声道再拼 WAV 头。这是唯一两端都收的格式。
 *
 * 采集用 ScriptProcessor 而非 AudioWorklet:后者要独立 worklet 文件,过 Vite 打包更麻烦,
 * 而「按住说完再识别」是批处理,对延迟不敏感,ScriptProcessor 足够。
 *
 * 权限:Capacitor 的 BridgeWebChromeClient 见到 AUDIO_CAPTURE 请求会自动弹安卓运行时授权
 * (前提是 AndroidManifest 声明了 RECORD_AUDIO,已补)。用户拒绝则 getUserMedia 抛错。
 */

/** 线性插值重采样到 16kHz(与桌面 webVoice.ts / voice_live.rs 同口径)。 */
function resampleTo16k(samples: Float32Array, inRate: number): Float32Array {
  if (inRate === 16000 || samples.length === 0) return samples;
  const ratio = 16000 / inRate;
  const outLen = Math.floor(samples.length * ratio);
  const out = new Float32Array(outLen);
  const last = samples.length - 1;
  for (let i = 0; i < outLen; i++) {
    const src = i / ratio;
    const i0 = Math.floor(src);
    const i1 = Math.min(i0 + 1, last);
    const frac = src - i0;
    out[i] = samples[i0] * (1 - frac) + samples[i1] * frac;
  }
  return out;
}

/** Float32 [-1,1] 单声道 → 16-bit PCM WAV 字节(44 字节头 + 数据)。 */
function encodeWav16(samples: Float32Array, sampleRate: number): ArrayBuffer {
  const dataLen = samples.length * 2;
  const buf = new ArrayBuffer(44 + dataLen);
  const dv = new DataView(buf);
  const wstr = (off: number, s: string) => {
    for (let i = 0; i < s.length; i++) dv.setUint8(off + i, s.charCodeAt(i));
  };
  wstr(0, "RIFF");
  dv.setUint32(4, 36 + dataLen, true);
  wstr(8, "WAVE");
  wstr(12, "fmt ");
  dv.setUint32(16, 16, true); // fmt chunk size
  dv.setUint16(20, 1, true); // PCM
  dv.setUint16(22, 1, true); // 单声道
  dv.setUint32(24, sampleRate, true);
  dv.setUint32(28, sampleRate * 2, true); // byte rate
  dv.setUint16(32, 2, true); // block align
  dv.setUint16(34, 16, true); // bits per sample
  wstr(36, "data");
  dv.setUint32(40, dataLen, true);
  let off = 44;
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    dv.setInt16(off, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    off += 2;
  }
  return buf;
}

/** ArrayBuffer → base64(分块转,避免 String.fromCharCode 一次吃几十万参数爆栈)。 */
function toBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  const CHUNK = 0x8000;
  let bin = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

/** 录音过短(< 0.3s)当误触,不发去识别。 */
const MIN_SECONDS = 0.3;

/**
 * 单次录音硬顶(秒)。**不是随便定的**:音频要 base64 塞进 `/api/invoke` 的 JSON body,
 * 而那条路由的 body 上限是 2MB(server.rs / hosting.rs 都是,且是有意收紧的安全默认)。
 * 16kHz/16bit/单声道 = 32KB/s,base64 后约 42.7KB/s → 2MB ≈ 47 秒。
 * 取 45 秒留出 JSON 外壳的余量;超了要在客户端自动收尾,而不是让用户说满一分钟
 * 再吃一个看不懂的 413。
 */
export const MAX_SECONDS = 45;

export class VoiceRecorder {
  private ctx: AudioContext | null = null;
  private stream: MediaStream | null = null;
  private node: ScriptProcessorNode | null = null;
  private src: MediaStreamAudioSourceNode | null = null;
  private chunks: Float32Array[] = [];
  private inRate = 16000;
  /** start() 在途(getUserMedia 还没 resolve)—— 重入闸。 */
  private starting = false;
  /** 最近一帧的音量(0~1),给 UI 画声波用。 */
  level = 0;
  /** 已录时长(秒),UI 用来显示倒计时并在到顶时自动收尾。 */
  elapsed = 0;

  get recording(): boolean {
    return this.node !== null;
  }

  async start(): Promise<void> {
    if (!navigator.mediaDevices?.getUserMedia) {
      throw new Error("这台设备不支持麦克风采集");
    }
    // 拒绝重入:getUserMedia 是异步的(首次还要等系统权限框),期间再来一发会把
    // this.stream 覆盖掉 —— 先到的那条 MediaStream 就此无人持有,getTracks().stop()
    // 永远调不到,麦克风一直被占着(状态栏红点常亮)直到杀进程。
    if (this.starting || this.stream) throw new Error("正在录音");
    this.starting = true;
    try {
      this.stream = await navigator.mediaDevices.getUserMedia({
        audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
      });
    } catch (e) {
      this.starting = false;
      const name = (e as DOMException)?.name ?? "";
      if (name === "NotAllowedError" || name === "SecurityError") {
        throw new Error("麦克风权限被拒绝 —— 去系统设置里把「北极星」的麦克风权限打开");
      }
      if (name === "NotFoundError") throw new Error("找不到麦克风");
      throw new Error(`打不开麦克风:${(e as Error).message || name}`);
    }
    this.starting = false;
    const Ctor =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    this.ctx = new Ctor();
    this.inRate = this.ctx.sampleRate;
    this.src = this.ctx.createMediaStreamSource(this.stream);
    const proc = this.ctx.createScriptProcessor(4096, 1, 1);
    this.chunks = [];
    this.elapsed = 0;
    let samples = 0;
    proc.onaudioprocess = (e) => {
      // 必须拷一份:inputBuffer 会被复用,存引用会被后续帧覆盖成一片相同的噪声。
      const frame = new Float32Array(e.inputBuffer.getChannelData(0));
      this.chunks.push(frame);
      samples += frame.length;
      this.elapsed = samples / this.inRate;
      let peak = 0;
      for (let i = 0; i < frame.length; i += 8) {
        const v = Math.abs(frame[i]);
        if (v > peak) peak = v;
      }
      // 平滑一下,免得音量条抖成筛子
      this.level = this.level * 0.6 + Math.min(1, peak * 2.2) * 0.4;
    };
    this.src.connect(proc);
    proc.connect(this.ctx.destination); // 不连目的地,部分 WebView 不触发回调
    this.node = proc;
  }

  /** 停止采集并释放设备 → { base64, seconds };录音过短返回 null(误触)。 */
  async stop(): Promise<{ base64: string; seconds: number } | null> {
    const { node, src, ctx, stream, inRate } = this;
    const chunks = this.chunks;
    this.node = null;
    this.src = null;
    this.ctx = null;
    this.stream = null;
    this.chunks = [];
    this.level = 0;
    this.elapsed = 0;
    this.starting = false;
    try {
      node?.disconnect();
      src?.disconnect();
    } catch {
      /* ignore */
    }
    stream?.getTracks().forEach((t) => t.stop());
    try {
      await ctx?.close();
    } catch {
      /* ignore */
    }

    const total = chunks.reduce((n, c) => n + c.length, 0);
    const seconds = total / inRate;
    if (seconds < MIN_SECONDS) return null;
    const merged = new Float32Array(total);
    let off = 0;
    for (const c of chunks) {
      merged.set(c, off);
      off += c.length;
    }
    const wav = encodeWav16(resampleTo16k(merged, inRate), 16000);
    return { base64: toBase64(wav), seconds };
  }

  /** 取消:停采集、丢缓冲、不产出。 */
  cancel(): void {
    void this.stop().catch(() => {});
    this.chunks = [];
  }
}
