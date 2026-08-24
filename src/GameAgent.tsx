import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import "./MachineLearning.css";

export default function GameAgent() {
  const { t } = useTranslation();
  const [error, setError] = useState<string | null>(null);

  // --- Track A (live vision-model loop) ---
  const [gameAgentRunning, setGameAgentRunning] = useState(false);
  const [gameAgentModel, setGameAgentModel] = useState("llava");
  const [gameAgentPrompt, setGameAgentPrompt] = useState(
    "You are playing a game. Look at the screenshot and decide the single best next action. " +
      'Reply with ONLY a JSON object: {"action":"click","x":<int>,"y":<int>} or ' +
      '{"action":"key","key":"<name>"} or {"action":"wait"}.',
  );
  const [gameAgentBusy, setGameAgentBusy] = useState(false);

  // --- Track B (Deep RL) recording ---
  const [recording, setRecording] = useState(false);
  const [recordingSession, setRecordingSession] = useState("session-1");
  const [recordingOutputDir, setRecordingOutputDir] = useState("");
  const [recordingBusy, setRecordingBusy] = useState(false);

  // --- Track B: label / train-bc / train-rl / play ---
  const [pipelineSessionDir, setPipelineSessionDir] = useState("");
  const [labelBusy, setLabelBusy] = useState(false);
  const [labelOutput, setLabelOutput] = useState<string | null>(null);

  const [bcCheckpointOut, setBcCheckpointOut] = useState("policy.pt");
  const [bcEpochs, setBcEpochs] = useState(10);
  const [bcBusy, setBcBusy] = useState(false);
  const [bcOutput, setBcOutput] = useState<string | null>(null);

  const [rlCheckpointIn, setRlCheckpointIn] = useState("policy.pt");
  const [rlCheckpointOut, setRlCheckpointOut] = useState("policy-rl.pt");
  const [rlEpochs, setRlEpochs] = useState(5);
  const [rlBusy, setRlBusy] = useState(false);
  const [rlOutput, setRlOutput] = useState<string | null>(null);

  const [playCheckpoint, setPlayCheckpoint] = useState("policy.pt");
  const [playing, setPlaying] = useState(false);
  const [playBusy, setPlayBusy] = useState(false);

  useEffect(() => {
    invoke<boolean>("game_agent_status").then(setGameAgentRunning).catch((e) => setError(String(e)));
    invoke<boolean>("recording_status").then(setRecording).catch((e) => setError(String(e)));
    invoke<boolean>("play_status").then(setPlaying).catch((e) => setError(String(e)));
  }, []);

  async function handleStartGameAgent() {
    setError(null);
    setGameAgentBusy(true);
    try {
      await invoke("start_game_agent", { model: gameAgentModel, prompt: gameAgentPrompt });
      setGameAgentRunning(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setGameAgentBusy(false);
    }
  }

  async function handleStopGameAgent() {
    setError(null);
    try {
      await invoke("stop_game_agent");
      setGameAgentRunning(false);
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleBrowseRecordingOutputDir() {
    const dir = await openFolderPicker({ directory: true, multiple: false });
    if (typeof dir === "string") setRecordingOutputDir(dir);
  }

  async function handleStartRecording() {
    setError(null);
    setRecordingBusy(true);
    try {
      await invoke("start_recording_session", { session: recordingSession, outputDir: recordingOutputDir });
      setRecording(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setRecordingBusy(false);
    }
  }

  async function handleStopRecording() {
    setError(null);
    try {
      await invoke("stop_recording_session");
      setRecording(false);
      if (recordingOutputDir.trim() && recordingSession.trim()) {
        setPipelineSessionDir(`${recordingOutputDir}/${recordingSession}`);
      }
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleBrowseSessionDir() {
    const dir = await openFolderPicker({ directory: true, multiple: false });
    if (typeof dir === "string") setPipelineSessionDir(dir);
  }

  async function handleLabel() {
    setError(null);
    setLabelBusy(true);
    setLabelOutput(null);
    try {
      const output = await invoke<string>("label_recording_session", { sessionDir: pipelineSessionDir });
      setLabelOutput(output);
    } catch (err) {
      setError(String(err));
    } finally {
      setLabelBusy(false);
    }
  }

  async function handleTrainBc() {
    setError(null);
    setBcBusy(true);
    setBcOutput(null);
    try {
      const output = await invoke<string>("train_behavior_cloning", {
        sessionDir: pipelineSessionDir,
        checkpointOut: bcCheckpointOut,
        epochs: bcEpochs,
      });
      setBcOutput(output);
    } catch (err) {
      setError(String(err));
    } finally {
      setBcBusy(false);
    }
  }

  async function handleTrainRl() {
    setError(null);
    setRlBusy(true);
    setRlOutput(null);
    try {
      const output = await invoke<string>("train_reinforcement", {
        sessionDir: pipelineSessionDir,
        checkpointIn: rlCheckpointIn,
        checkpointOut: rlCheckpointOut,
        epochs: rlEpochs,
      });
      setRlOutput(output);
    } catch (err) {
      setError(String(err));
    } finally {
      setRlBusy(false);
    }
  }

  async function handleStartPlay() {
    setError(null);
    setPlayBusy(true);
    try {
      await invoke("start_play_checkpoint", { checkpoint: playCheckpoint });
      setPlaying(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setPlayBusy(false);
    }
  }

  async function handleStopPlay() {
    setError(null);
    try {
      await invoke("stop_play_checkpoint");
      setPlaying(false);
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="ml-page">
      <h1>{t("gameAgentPage.title")}</h1>
      {error && (
        <div className="acc-error" role="alert">
          {error}
          <button onClick={() => setError(null)} aria-label={t("acc.dismissError")}>×</button>
        </div>
      )}

      <section className="acc-section">
        <h2>{t("acc.gameAgent.heading")}</h2>
        <p className="acc-hint">{t("acc.gameAgent.warning")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("acc.gameAgent.modelPlaceholder")}
            value={gameAgentModel}
            onChange={(e) => setGameAgentModel(e.target.value)}
            disabled={gameAgentRunning}
          />
        </div>
        <textarea
          rows={3}
          value={gameAgentPrompt}
          onChange={(e) => setGameAgentPrompt(e.target.value)}
          disabled={gameAgentRunning}
        />
        <p>
          {t("acc.gameAgent.statusLabel")}{" "}
          {gameAgentRunning ? t("acc.gameAgent.statusRunning") : t("acc.gameAgent.statusStopped")}{" "}
          {gameAgentRunning ? (
            <button onClick={() => handleStopGameAgent()}>{t("acc.gameAgent.stop")}</button>
          ) : (
            <button disabled={gameAgentBusy} onClick={() => handleStartGameAgent()}>
              {gameAgentBusy ? t("acc.gameAgent.starting") : t("acc.gameAgent.start")}
            </button>
          )}
        </p>
      </section>

      <section className="acc-section">
        <h2>{t("acc.recording.heading")}</h2>
        <p className="acc-hint">{t("acc.recording.hint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("acc.recording.sessionNamePlaceholder")}
            value={recordingSession}
            onChange={(e) => setRecordingSession(e.target.value)}
            disabled={recording}
          />
          <input
            type="text"
            placeholder={t("acc.recording.outputDirectoryPlaceholder")}
            value={recordingOutputDir}
            onChange={(e) => setRecordingOutputDir(e.target.value)}
            disabled={recording}
          />
          <button disabled={recording} onClick={() => handleBrowseRecordingOutputDir()}>
            {t("acc.recording.browse")}
          </button>
        </div>
        <p>
          {t("acc.recording.statusLabel")}{" "}
          {recording ? t("acc.recording.statusRecording") : t("acc.recording.statusStopped")}{" "}
          {recording ? (
            <button onClick={() => handleStopRecording()}>{t("acc.recording.stop")}</button>
          ) : (
            <button disabled={recordingBusy || !recordingOutputDir.trim()} onClick={() => handleStartRecording()}>
              {recordingBusy ? t("acc.recording.starting") : t("acc.recording.startRecording")}
            </button>
          )}
        </p>
        {recordingOutputDir.trim() && recordingSession.trim() && (
          <p className="acc-hint">
            {t("acc.recording.sessionDirLabel")}{" "}
            <code>{`${recordingOutputDir}/${recordingSession}`}</code>{" "}
            <button
              onClick={() =>
                navigator.clipboard.writeText(`${recordingOutputDir}/${recordingSession}`).catch(() => {})
              }
            >
              {t("acc.recording.copyPath")}
            </button>
          </p>
        )}
      </section>

      <section className="acc-section">
        <h2>{t("ml.pipeline.sessionDirHeading")}</h2>
        <p className="acc-hint">{t("ml.pipeline.sessionDirHint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("ml.pipeline.sessionDirPlaceholder")}
            value={pipelineSessionDir}
            onChange={(e) => setPipelineSessionDir(e.target.value)}
          />
          <button onClick={() => handleBrowseSessionDir()}>{t("acc.recording.browse")}</button>
        </div>
      </section>

      <section className="acc-section">
        <h2>{t("ml.pipeline.labelHeading")}</h2>
        <p className="acc-hint">{t("ml.pipeline.labelHint")}</p>
        <button disabled={labelBusy || !pipelineSessionDir.trim()} onClick={() => handleLabel()}>
          {labelBusy ? t("ml.pipeline.running") : t("ml.pipeline.labelRun")}
        </button>
        {labelOutput !== null && <pre className="ml-pipeline-output">{labelOutput}</pre>}
      </section>

      <section className="acc-section">
        <h2>{t("ml.pipeline.bcHeading")}</h2>
        <p className="acc-hint">{t("ml.pipeline.bcHint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("ml.pipeline.checkpointOutPlaceholder")}
            value={bcCheckpointOut}
            onChange={(e) => setBcCheckpointOut(e.target.value)}
          />
          <input
            type="number"
            min={1}
            placeholder={t("ml.pipeline.epochsPlaceholder")}
            value={bcEpochs}
            onChange={(e) => setBcEpochs(Number(e.target.value) || 1)}
          />
        </div>
        <button
          disabled={bcBusy || !pipelineSessionDir.trim() || !bcCheckpointOut.trim()}
          onClick={() => handleTrainBc()}
        >
          {bcBusy ? t("ml.pipeline.running") : t("ml.pipeline.bcRun")}
        </button>
        {bcOutput !== null && <pre className="ml-pipeline-output">{bcOutput}</pre>}
      </section>

      <section className="acc-section">
        <h2>{t("ml.pipeline.rlHeading")}</h2>
        <p className="acc-hint">{t("ml.pipeline.rlHint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("ml.pipeline.checkpointInPlaceholder")}
            value={rlCheckpointIn}
            onChange={(e) => setRlCheckpointIn(e.target.value)}
          />
          <input
            type="text"
            placeholder={t("ml.pipeline.checkpointOutPlaceholder")}
            value={rlCheckpointOut}
            onChange={(e) => setRlCheckpointOut(e.target.value)}
          />
          <input
            type="number"
            min={1}
            placeholder={t("ml.pipeline.epochsPlaceholder")}
            value={rlEpochs}
            onChange={(e) => setRlEpochs(Number(e.target.value) || 1)}
          />
        </div>
        <button
          disabled={rlBusy || !pipelineSessionDir.trim() || !rlCheckpointIn.trim() || !rlCheckpointOut.trim()}
          onClick={() => handleTrainRl()}
        >
          {rlBusy ? t("ml.pipeline.running") : t("ml.pipeline.rlRun")}
        </button>
        {rlOutput !== null && <pre className="ml-pipeline-output">{rlOutput}</pre>}
      </section>

      <section className="acc-section">
        <h2>{t("ml.pipeline.playHeading")}</h2>
        <p className="acc-hint">{t("ml.pipeline.playHint")}</p>
        <div className="acc-form-row">
          <input
            type="text"
            placeholder={t("ml.pipeline.checkpointInPlaceholder")}
            value={playCheckpoint}
            onChange={(e) => setPlayCheckpoint(e.target.value)}
            disabled={playing}
          />
        </div>
        <p>
          {t("acc.gameAgent.statusLabel")}{" "}
          {playing ? t("acc.gameAgent.statusRunning") : t("acc.gameAgent.statusStopped")}{" "}
          {playing ? (
            <button onClick={() => handleStopPlay()}>{t("acc.gameAgent.stop")}</button>
          ) : (
            <button disabled={playBusy || !playCheckpoint.trim()} onClick={() => handleStartPlay()}>
              {playBusy ? t("acc.gameAgent.starting") : t("ml.pipeline.playRun")}
            </button>
          )}
        </p>
      </section>
    </div>
  );
}
