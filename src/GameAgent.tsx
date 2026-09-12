import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import { Icon } from "./shell/Icons";
import "./styles/screens/game-agent.css";

/** What a pipeline stage can be, and what this app is able to know.
 *
 *  The design has a fourth state, "done", with a tick and a Done badge.
 *  It is not used here. Nothing on disk records that a stage has ever
 *  run — `label_recording_session` and the two training commands return
 *  their output and keep no state — so "done" could only mean "ran while
 *  this window has been open", and a stage that forgets its own history
 *  every restart should not be claiming completion. A stage that has
 *  produced output in this session shows the output; that is a fact, and
 *  it is the one worth showing. */
type StageState = "blocked" | "running" | "ready";

function Stage({
  index,
  state,
  title,
  description,
  output,
  children,
}: {
  index: number;
  state: StageState;
  title: string;
  description: string;
  output?: string | null;
  children: ReactNode;
}) {
  return (
    <div className="stage" data-state={state}>
      <span className="stage-num" aria-hidden="true">
        {state === "running" ? <Icon name="chevron" size="sm" /> : index}
      </span>
      <div className="stage-body">
        <h3>{title}</h3>
        <p>{description}</p>
        {output != null && <div className="stage-out">{output}</div>}
      </div>
      <div className="stage-actions">{children}</div>
    </div>
  );
}

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

  const hasSessionDir = pipelineSessionDir.trim() !== "";
  /* Blocked means a required input is missing, which is checkable. It does
     not mean "the previous stage has not run" — that is exactly what this
     app cannot know. */
  const labelState: StageState = labelBusy ? "running" : hasSessionDir ? "ready" : "blocked";
  const bcState: StageState = bcBusy ? "running" : hasSessionDir && bcCheckpointOut.trim() ? "ready" : "blocked";
  const rlState: StageState = rlBusy
    ? "running"
    : hasSessionDir && rlCheckpointIn.trim() && rlCheckpointOut.trim()
      ? "ready"
      : "blocked";
  const playState: StageState = playing || playBusy ? "running" : playCheckpoint.trim() ? "ready" : "blocked";
  const recordState: StageState = recording || recordingBusy ? "running" : recordingOutputDir.trim() ? "ready" : "blocked";

  return (
    <>
      <div className="workspace-header">
        <span className="workspace-title">{t("gameAgentPage.title")}</span>
        <span className="badge" data-kind="local">
          {t("gameAgentPage.localOnly")}
        </span>
      </div>

      <div className="workspace-body">
        <div className="pane">
          {error && (
            <div className="callout" data-kind="danger" role="alert">
              <div className="callout-body">{error}</div>
            </div>
          )}

          <p className="pane-intro">{t("gameAgentPage.intro")}</p>

          <div className="callout" data-kind="warning">
            <Icon name="alert" />
            <div className="callout-body">
              <strong>{t("gameAgentPage.takeoverTitle")}</strong>
              {t("acc.gameAgent.warning")}
            </div>
          </div>

          {/* Track A is not part of the pipeline: it is a different way of
              playing (a vision model deciding each move live) rather than a
              stage on the way to a trained policy. Keeping it above the
              numbered sequence says that, where putting it inside would
              imply it feeds the next stage. */}
          <section>
            <div className="section-head">
              <h2>{t("acc.gameAgent.heading")}</h2>
              <span className="count">{t("gameAgentPage.trackA")}</span>
            </div>
            <div className="card card-pad">
              <div className="field">
                <label className="field-label" htmlFor="ga-model">
                  {t("acc.gameAgent.modelPlaceholder")}
                </label>
                <input
                  id="ga-model"
                  className="input"
                  type="text"
                  value={gameAgentModel}
                  onChange={(e) => setGameAgentModel(e.target.value)}
                  disabled={gameAgentRunning}
                />
              </div>
              <div className="field">
                <label className="field-label" htmlFor="ga-prompt">
                  {t("gameAgentPage.promptLabel")}
                </label>
                <textarea
                  id="ga-prompt"
                  className="textarea"
                  rows={3}
                  value={gameAgentPrompt}
                  onChange={(e) => setGameAgentPrompt(e.target.value)}
                  disabled={gameAgentRunning}
                />
              </div>
              <div className="stage-actions">
                <span className="status-line">
                  <span className="status-dot" data-state={gameAgentRunning ? "running" : "idle"} aria-hidden="true" />
                  {gameAgentRunning ? t("acc.gameAgent.statusRunning") : t("acc.gameAgent.statusStopped")}
                </span>
                {gameAgentRunning ? (
                  <button className="btn btn-danger btn-sm" type="button" onClick={() => handleStopGameAgent()}>
                    {t("acc.gameAgent.stop")}
                  </button>
                ) : (
                  <button
                    className="btn btn-primary btn-sm"
                    type="button"
                    disabled={gameAgentBusy}
                    onClick={() => handleStartGameAgent()}
                  >
                    {gameAgentBusy ? t("acc.gameAgent.starting") : t("acc.gameAgent.start")}
                  </button>
                )}
              </div>
            </div>
          </section>

          <section>
            <div className="section-head">
              <h2>{t("gameAgentPage.pipeline")}</h2>
              <span className="count">{t("gameAgentPage.trackB")}</span>
            </div>

            <div className="card card-pad">
              <div className="field">
                <label className="field-label" htmlFor="ga-session-dir">
                  {t("ml.pipeline.sessionDirHeading")}
                </label>
                <div className="searchbar">
                  <input
                    id="ga-session-dir"
                    className="input"
                    type="text"
                    placeholder={t("ml.pipeline.sessionDirPlaceholder")}
                    value={pipelineSessionDir}
                    onChange={(e) => setPipelineSessionDir(e.target.value)}
                  />
                  <button className="btn btn-secondary" type="button" onClick={() => handleBrowseSessionDir()}>
                    {t("acc.recording.browse")}
                  </button>
                </div>
                <p className="field-hint">{t("ml.pipeline.sessionDirHint")}</p>
              </div>
            </div>

            <div className="card card-pad">
              <div className="stages">
                <Stage
                  index={1}
                  state={recordState}
                  title={t("acc.recording.heading")}
                  description={t("acc.recording.hint")}
                  output={
                    recordingOutputDir.trim() && recordingSession.trim()
                      ? `${recordingOutputDir}/${recordingSession}`
                      : null
                  }
                >
                  <input
                    className="input input-mono"
                    type="text"
                    aria-label={t("acc.recording.sessionNamePlaceholder")}
                    placeholder={t("acc.recording.sessionNamePlaceholder")}
                    value={recordingSession}
                    onChange={(e) => setRecordingSession(e.target.value)}
                    disabled={recording}
                  />
                  <button
                    className="btn btn-ghost btn-sm"
                    type="button"
                    disabled={recording}
                    onClick={() => handleBrowseRecordingOutputDir()}
                  >
                    {t("acc.recording.browse")}
                  </button>
                  {recording ? (
                    <button className="btn btn-danger btn-sm" type="button" onClick={() => handleStopRecording()}>
                      {t("acc.recording.stop")}
                    </button>
                  ) : (
                    <button
                      className="btn btn-primary btn-sm"
                      type="button"
                      disabled={recordingBusy || !recordingOutputDir.trim()}
                      onClick={() => handleStartRecording()}
                    >
                      {recordingBusy ? t("acc.recording.starting") : t("acc.recording.startRecording")}
                    </button>
                  )}
                </Stage>

                <Stage
                  index={2}
                  state={labelState}
                  title={t("ml.pipeline.labelHeading")}
                  description={t("ml.pipeline.labelHint")}
                  output={labelOutput}
                >
                  <button
                    className="btn btn-secondary btn-sm"
                    type="button"
                    disabled={labelBusy || !hasSessionDir}
                    onClick={() => handleLabel()}
                  >
                    {labelBusy ? t("ml.pipeline.running") : t("ml.pipeline.labelRun")}
                  </button>
                </Stage>

                <Stage
                  index={3}
                  state={bcState}
                  title={t("ml.pipeline.bcHeading")}
                  description={t("ml.pipeline.bcHint")}
                  output={bcOutput}
                >
                  <input
                    className="input input-mono"
                    type="text"
                    aria-label={t("ml.pipeline.checkpointOutPlaceholder")}
                    placeholder={t("ml.pipeline.checkpointOutPlaceholder")}
                    value={bcCheckpointOut}
                    onChange={(e) => setBcCheckpointOut(e.target.value)}
                  />
                  <input
                    className="input input-num"
                    type="number"
                    min={1}
                    aria-label={t("ml.pipeline.epochsPlaceholder")}
                    value={bcEpochs}
                    onChange={(e) => setBcEpochs(Number(e.target.value) || 1)}
                  />
                  <button
                    className="btn btn-secondary btn-sm"
                    type="button"
                    disabled={bcBusy || !hasSessionDir || !bcCheckpointOut.trim()}
                    onClick={() => handleTrainBc()}
                  >
                    {bcBusy ? t("ml.pipeline.running") : t("ml.pipeline.bcRun")}
                  </button>
                </Stage>

                <Stage
                  index={4}
                  state={rlState}
                  title={t("ml.pipeline.rlHeading")}
                  description={t("ml.pipeline.rlHint")}
                  output={rlOutput}
                >
                  <input
                    className="input input-mono"
                    type="text"
                    aria-label={t("ml.pipeline.checkpointInPlaceholder")}
                    placeholder={t("ml.pipeline.checkpointInPlaceholder")}
                    value={rlCheckpointIn}
                    onChange={(e) => setRlCheckpointIn(e.target.value)}
                  />
                  <input
                    className="input input-mono"
                    type="text"
                    aria-label={t("ml.pipeline.checkpointOutPlaceholder")}
                    placeholder={t("ml.pipeline.checkpointOutPlaceholder")}
                    value={rlCheckpointOut}
                    onChange={(e) => setRlCheckpointOut(e.target.value)}
                  />
                  <input
                    className="input input-num"
                    type="number"
                    min={1}
                    aria-label={t("ml.pipeline.epochsPlaceholder")}
                    value={rlEpochs}
                    onChange={(e) => setRlEpochs(Number(e.target.value) || 1)}
                  />
                  <button
                    className="btn btn-secondary btn-sm"
                    type="button"
                    disabled={rlBusy || !hasSessionDir || !rlCheckpointIn.trim() || !rlCheckpointOut.trim()}
                    onClick={() => handleTrainRl()}
                  >
                    {rlBusy ? t("ml.pipeline.running") : t("ml.pipeline.rlRun")}
                  </button>
                </Stage>

                <Stage
                  index={5}
                  state={playState}
                  title={t("ml.pipeline.playHeading")}
                  description={t("ml.pipeline.playHint")}
                >
                  <input
                    className="input input-mono"
                    type="text"
                    aria-label={t("ml.pipeline.checkpointInPlaceholder")}
                    placeholder={t("ml.pipeline.checkpointInPlaceholder")}
                    value={playCheckpoint}
                    onChange={(e) => setPlayCheckpoint(e.target.value)}
                    disabled={playing}
                  />
                  {playing ? (
                    <button className="btn btn-danger btn-sm" type="button" onClick={() => handleStopPlay()}>
                      {t("acc.gameAgent.stop")}
                    </button>
                  ) : (
                    <button
                      className="btn btn-primary btn-sm"
                      type="button"
                      disabled={playBusy || !playCheckpoint.trim()}
                      onClick={() => handleStartPlay()}
                    >
                      {playBusy ? t("acc.gameAgent.starting") : t("ml.pipeline.playRun")}
                    </button>
                  )}
                </Stage>
              </div>
            </div>
          </section>
        </div>
      </div>
    </>
  );
}
