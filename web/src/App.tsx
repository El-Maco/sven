'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  CircleAlert,
  Clock,
  LoaderCircle,
  Square,
  Wifi,
  WifiOff,
  X,
} from 'lucide-react';
import * as api from './api';
import { useDeskState } from './useDeskState';
import {
  SvenCommand,
  SvenDirection,
  SvenMoveMode,
  type PositionEntry,
  type SvenInfo,
  type SvenResponse,
} from './types';

const DURATIONS = [
  { label: '1 s', value: 1000 },
  { label: '2 s', value: 2000 },
  { label: '5 s', value: 5000 },
  { label: '10 s', value: 10000 },
  { label: '15 s', value: 15000 },
];

const DISTANCES = [
  { label: '1 cm', value: 10 },
  { label: '2 cm', value: 20 },
  { label: '5 cm', value: 50 },
  { label: '10 cm', value: 100 },
  { label: '15 cm', value: 150 },
  { label: '20 cm', value: 200 },
  { label: '25 cm', value: 250 },
  { label: '30 cm', value: 300 },
];

const STATUS_TIMEOUT_MS = 4000;

function centimetres(mm: number | null | undefined): string {
  if (mm === null || mm === undefined) return '--';
  return (mm / 10).toFixed(1);
}

export default function App() {
  const { state, connection } = useDeskState();
  const [info, setInfo] = useState<SvenInfo | null>(null);
  const [selectedValue, setSelectedValue] = useState<number>(800);
  const [selectedDirection, setSelectedDirection] = useState<SvenDirection>(
    SvenDirection.Up,
  );
  const [showDirectionButtons, setShowDirectionButtons] = useState(false);
  const [isSending, setIsSending] = useState(false);
  const [selectedMode, setSelectedMode] = useState<SvenMoveMode>(
    SvenMoveMode.Duration,
  );
  const [calibrationSwitch, setCalibrationSwitch] = useState(false);
  const [positions, setPositions] = useState<PositionEntry[]>([]);
  const [newPositionName, setNewPositionName] = useState('');
  const [notification, setNotification] = useState<SvenResponse | null>(null);
  const notificationTimeout = useRef<number | null>(null);

  const busy = isSending || state?.moving === true;
  const isOnline = connection === 'online';
  const calibrated = state?.calibrated === true;

  const loadPositions = useCallback(async () => {
    try {
      setPositions(await api.fetchPositions());
    } catch {
      setPositions([]);
    }
  }, []);

  useEffect(() => {
    api
      .fetchInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
    void loadPositions();
  }, [loadPositions]);

  useEffect(() => {
    if (state?.calibrated && state.height_mm !== undefined) {
      setSelectedValue(state.height_mm);
    }
  }, [state?.calibrated, state?.height_mm]);

  useEffect(() => {
    return () => {
      if (notificationTimeout.current !== null) {
        window.clearTimeout(notificationTimeout.current);
      }
    };
  }, []);

  const clearNotification = useCallback(() => {
    if (notificationTimeout.current !== null) {
      window.clearTimeout(notificationTimeout.current);
      notificationTimeout.current = null;
    }
    setNotification(null);
  }, []);

  const showNotification = useCallback(
    (next: Omit<SvenResponse, 'timestamp'>) => {
      clearNotification();
      setNotification({
        ...next,
        timestamp: new Date().toLocaleTimeString(),
      });
      notificationTimeout.current = window.setTimeout(() => {
        notificationTimeout.current = null;
        setNotification(null);
      }, STATUS_TIMEOUT_MS);
    },
    [clearNotification],
  );

  const handleDirectionClick = (direction: SvenDirection) => {
    if (busy) return;
    clearNotification();
    setSelectedDirection(direction);
    setShowDirectionButtons(true);
  };

  const getSelectedCommand = (mode: SvenMoveMode): SvenCommand | null => {
    if (mode === SvenMoveMode.Duration) {
      return selectedDirection === SvenDirection.Up
        ? SvenCommand.UpDuration
        : SvenCommand.DownDuration;
    }
    if (mode === SvenMoveMode.Relative) {
      return selectedDirection === SvenDirection.Up
        ? SvenCommand.UpRelative
        : SvenCommand.DownRelative;
    }
    if (mode === SvenMoveMode.Absolute) {
      return SvenCommand.AbsoluteHeight;
    }
    if (mode === SvenMoveMode.Position) {
      return calibrationSwitch ? SvenCommand.Calibrate : SvenCommand.Position;
    }
    return null;
  };

  const sendCommand = async (value: number, moveMode?: SvenMoveMode) => {
    const mode = moveMode ?? selectedMode;
    const command = getSelectedCommand(mode);
    if (!command || busy) return;

    const needsCalibration =
      command === SvenCommand.AbsoluteHeight ||
      command === SvenCommand.UpRelative ||
      command === SvenCommand.DownRelative ||
      (command === SvenCommand.Position && !calibrationSwitch);

    if (needsCalibration && !calibrated) {
      showNotification({
        success: false,
        error: 'Calibrate the desk first so the height estimate is known',
      });
      return;
    }

    setIsSending(true);
    clearNotification();

    try {
      await api.sendCommand(command, value);
      if (command === SvenCommand.Calibrate) {
        setCalibrationSwitch(false);
      }
      showNotification({
        success: true,
        data: { command, value },
      });
      setShowDirectionButtons(false);
    } catch (error) {
      showNotification({
        success: false,
        error:
          error instanceof Error ? error.message : 'The desk did not answer',
      });
    } finally {
      setIsSending(false);
    }
  };

  const handleStop = async () => {
    try {
      await api.stopDesk();
      showNotification({ success: true, data: { command: 'Stop' } });
    } catch (error) {
      showNotification({
        success: false,
        error: error instanceof Error ? error.message : 'Stop failed',
      });
    }
  };

  const savePosition = async () => {
    const name = newPositionName.trim();
    if (!name || !calibrated || busy) return;
    try {
      await api.addPosition(name, state?.height_mm ?? 0);
      setNewPositionName('');
      await loadPositions();
      showNotification({ success: true, data: { position: name } });
    } catch (error) {
      showNotification({
        success: false,
        error: error instanceof Error ? error.message : 'Could not save position',
      });
    }
  };

  const deletePosition = async (name: string) => {
    if (busy) return;
    try {
      await api.removePosition(name);
      await loadPositions();
      showNotification({ success: true, data: { removed: name } });
    } catch (error) {
      showNotification({
        success: false,
        error:
          error instanceof Error ? error.message : 'Could not remove position',
      });
    }
  };

  const positionButtons = [
    { name: 'Bottom', height_mm: 622, index: 0, fixed: true },
    { name: 'Top', height_mm: 1274, index: 1, fixed: true },
    ...positions.map((position, index) => ({
      ...position,
      index: index + 2,
      fixed: false,
    })),
  ];

  const reset = () => setShowDirectionButtons(false);

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900">
      <div className="container mx-auto px-4 py-8 pb-[calc(2rem+env(safe-area-inset-bottom))]">
        <div className="mx-auto max-w-md space-y-6">
          <header className="mb-8 text-center">
            <div className="mb-2 flex items-center justify-center gap-3">
              <h1 className="text-3xl font-bold text-white">Sven</h1>
              <span
                className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium ${
                  isOnline
                    ? 'border-emerald-400/30 bg-emerald-400/10 text-emerald-200'
                    : connection === 'connecting'
                      ? 'border-amber-400/30 bg-amber-400/10 text-amber-200'
                      : 'border-red-400/30 bg-red-400/10 text-red-200'
                }`}
              >
                {isOnline ? <Wifi size={12} /> : <WifiOff size={12} />}
                {connection}
              </span>
            </div>
            <p className="font-mono text-sm text-slate-300">
              {calibrated ? centimetres(state?.height_mm) : '--'} cm
              {calibrated &&
                state?.target_mm !== null &&
                state?.target_mm !== undefined && (
                <span className="text-slate-400">
                  {' '}
                  &rarr; {centimetres(state.target_mm)} cm
                </span>
              )}
            </p>
            {busy && (
              <div className="mt-3 flex items-center justify-center gap-2 text-sm text-slate-300">
                <LoaderCircle className="animate-spin" size={16} />
                {state?.moving ? 'moving' : 'sending'}
              </div>
            )}
          </header>

          {!isOnline && connection === 'offline' && (
            <div className="rounded-xl border border-red-400/30 bg-red-500/10 p-3 text-sm text-red-200">
              Desk unreachable. Check that it is powered and on the same network,
              or that Tailscale is connected.
            </div>
          )}

          {!calibrated && isOnline && (
            <div className="rounded-xl border border-amber-400/30 bg-amber-400/10 p-4 text-sm text-amber-100">
              <p className="font-semibold">Position unknown</p>
              <p className="mt-1 text-amber-200/90">
                The desk has not been calibrated since it booted. Enable
                calibration mode and choose a position to anchor the height
                estimate.
              </p>
              <button
                onClick={() => setCalibrationSwitch(true)}
                className="mt-3 rounded-lg border border-amber-300/40 bg-amber-300/10 px-3 py-2 font-medium text-amber-50 transition hover:bg-amber-300/20"
              >
                Enable calibration mode
              </button>
            </div>
          )}

          {notification && (
            <div
              aria-live="polite"
              className={`toast-enter fixed right-4 top-4 z-50 max-w-sm cursor-pointer rounded-xl border-2 p-4 shadow-2xl ${
                notification.success
                  ? 'border-emerald-400/60 bg-emerald-500/90 text-emerald-50'
                  : 'border-red-400/60 bg-red-500/90 text-red-50'
              }`}
              onClick={clearNotification}
            >
              <div className="mb-1 flex items-center gap-2">
                {notification.success ? (
                  <CheckCircle2 size={16} />
                ) : (
                  <CircleAlert size={16} />
                )}
                <span className="font-semibold">
                  {notification.success ? 'Command accepted' : 'Error'}
                </span>
                <span className="ml-auto text-xs opacity-75">
                  {notification.timestamp}
                </span>
              </div>
              <div className="font-mono text-xs opacity-90">
                {notification.success
                  ? JSON.stringify(notification.data)
                  : notification.error}
              </div>
            </div>
          )}

          {busy && (
            <button
              onClick={handleStop}
              className="flex w-full items-center justify-center gap-2 rounded-xl border-2 border-red-300/40 bg-red-500/20 py-3 font-semibold text-red-100 transition hover:bg-red-500/30 active:scale-[0.99]"
            >
              <Square size={18} fill="currentColor" />
              Stop
            </button>
          )}

          <section className="space-y-4 rounded-2xl border border-white/20 bg-white/10 p-6 shadow-2xl backdrop-blur-lg">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold text-white">
                Move to a position
              </h2>
              <p className="font-mono text-sm text-slate-300">
                h: {calibrated ? centimetres(state?.height_mm) : '--'} cm
              </p>
            </div>

            <div className="flex flex-col gap-2 text-sm text-slate-300">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <p className="font-mono text-xs uppercase tracking-wide text-slate-400">
                    Calibration mode
                  </p>
                  <p className="mt-1 text-xs text-slate-400">
                    Home to the nearest end, then move to the selected position.
                  </p>
                </div>
                <button
                  onClick={() => setCalibrationSwitch((value) => !value)}
                  aria-pressed={calibrationSwitch}
                  aria-label="Toggle calibration mode"
                  className={`relative h-6 w-10 shrink-0 rounded-full border-none p-0 transition-colors duration-200 focus:outline-none focus:ring-2 focus:ring-white/40 ${
                    calibrationSwitch ? 'bg-emerald-400' : 'bg-gray-400'
                  }`}
                >
                  <span
                    className={`absolute top-0.5 block h-5 w-5 rounded-full bg-white transition-all duration-200 ${
                      calibrationSwitch ? 'left-4.5' : 'left-0.5'
                    }`}
                  />
                </button>
              </div>
            </div>

            <div className="grid grid-cols-2 gap-3">
              {positionButtons.map((position) => (
                <div key={position.name} className="relative">
                  <button
                    onClick={() =>
                      sendCommand(position.index, SvenMoveMode.Position)
                    }
                    disabled={busy || (!calibrated && !calibrationSwitch)}
                    className={`flex w-full items-center justify-center gap-2 rounded-lg border-2 p-3 pr-8 transition-all duration-200 ${
                      state?.position === position.name
                        ? 'border-blue-400 bg-blue-900/20 text-blue-200'
                        : 'border-white/20 bg-white/5 text-slate-300 hover:border-white/40 hover:bg-white/10'
                    } disabled:cursor-not-allowed disabled:opacity-40`}
                  >
                    <Clock size={16} />
                    <span className="truncate">{position.name}</span>
                  </button>
                  {!position.fixed && (
                    <button
                      onClick={() => deletePosition(position.name)}
                      disabled={busy}
                      aria-label={`Remove ${position.name}`}
                      className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-1 text-slate-400 hover:bg-white/10 hover:text-red-200 disabled:opacity-40"
                    >
                      <X size={12} />
                    </button>
                  )}
                </div>
              ))}
            </div>

            <form
              className="flex items-center gap-2"
              onSubmit={(event) => {
                event.preventDefault();
                void savePosition();
              }}
            >
              <input
                value={newPositionName}
                onChange={(event) => setNewPositionName(event.target.value)}
                placeholder="New position name"
                maxLength={24}
                disabled={busy || !calibrated}
                className="min-w-0 flex-1 rounded-lg border-2 border-white/20 bg-white/5 p-2.5 text-sm text-slate-200 placeholder:text-slate-500 focus:border-blue-400 focus:outline-none disabled:opacity-40"
              />
              <button
                type="submit"
                disabled={busy || !calibrated || !newPositionName.trim()}
                className="rounded-lg border-2 border-emerald-400/40 bg-emerald-400/10 px-3 py-2.5 text-sm font-medium text-emerald-100 transition hover:bg-emerald-400/20 disabled:cursor-not-allowed disabled:opacity-40"
              >
                Save height
              </button>
            </form>
          </section>

          <section className="space-y-4 rounded-2xl border border-white/20 bg-white/10 p-6 shadow-2xl backdrop-blur-lg">
            <div className="mb-2 flex flex-row items-center justify-center gap-3">
              <button
                className={`rounded-lg px-4 py-2 ${
                  selectedMode === SvenMoveMode.Relative
                    ? 'bg-blue-500 text-white'
                    : 'bg-white/10 text-slate-300 hover:bg-white/20'
                }`}
                onClick={() => setSelectedMode(SvenMoveMode.Relative)}
              >
                Distance
              </button>
              <button
                className={`rounded-lg px-4 py-2 ${
                  selectedMode === SvenMoveMode.Duration
                    ? 'bg-blue-500 text-white'
                    : 'bg-white/10 text-slate-300 hover:bg-white/20'
                }`}
                onClick={() => setSelectedMode(SvenMoveMode.Duration)}
              >
                Duration
              </button>
            </div>

            {!showDirectionButtons && (
              <>
                <h2 className="mb-4 text-lg font-semibold text-white">
                  Move in a direction
                </h2>
                <button
                  onClick={() => handleDirectionClick(SvenDirection.Up)}
                  disabled={busy}
                  className="mb-3 flex w-full items-center justify-center gap-3 rounded-xl bg-gradient-to-r from-green-500 to-emerald-600 px-6 py-4 font-semibold text-white shadow-lg transition-all duration-200 hover:from-green-600 hover:to-emerald-700 active:scale-[0.99] disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <ChevronUp size={24} />
                  UP
                </button>
                <button
                  onClick={() => handleDirectionClick(SvenDirection.Down)}
                  disabled={busy}
                  className="flex w-full items-center justify-center gap-3 rounded-xl bg-gradient-to-r from-red-500 to-rose-600 px-6 py-4 font-semibold text-white shadow-lg transition-all duration-200 hover:from-red-600 hover:to-rose-700 active:scale-[0.99] disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <ChevronDown size={24} />
                  DOWN
                </button>
              </>
            )}

            {showDirectionButtons && (
              <>
                <div className="mb-4 flex items-center justify-between">
                  <h2 className="text-lg font-semibold text-white">
                    {SvenMoveMode[selectedMode]} move
                  </h2>
                  <button
                    onClick={reset}
                    className="text-sm text-slate-300 underline hover:text-white"
                  >
                    Change direction
                  </button>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  {(selectedMode === SvenMoveMode.Duration
                    ? DURATIONS
                    : DISTANCES
                  ).map((option) => (
                    <button
                      key={option.value}
                      onClick={() => sendCommand(option.value)}
                      disabled={
                        busy ||
                        (selectedMode === SvenMoveMode.Relative && !calibrated)
                      }
                      className="flex items-center justify-center gap-2 rounded-lg border-2 border-white/20 bg-white/5 p-3 text-slate-300 transition-all duration-200 hover:border-white/40 hover:bg-white/10 disabled:cursor-not-allowed disabled:opacity-40"
                    >
                      <Clock size={16} />
                      {option.label}
                    </button>
                  ))}
                </div>
              </>
            )}
          </section>

          <section className="space-y-4 rounded-2xl border border-white/20 bg-white/10 p-6 shadow-2xl backdrop-blur-lg">
            <h2 className="text-lg font-semibold text-white">Move absolute</h2>
            {!calibrated ? (
              <p className="text-sm text-slate-400">
                Calibrate the desk first to enable absolute moves.
              </p>
            ) : (
              <form
                className="flex items-center gap-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  sendCommand(selectedValue, SvenMoveMode.Absolute);
                }}
              >
                <input
                  type="range"
                  min={622}
                  max={1274}
                  value={selectedValue}
                  onChange={(event) => setSelectedValue(Number(event.target.value))}
                  disabled={busy}
                  aria-label="Target height in millimetres"
                  className="flex-1 rounded-lg border-2 border-white/20 bg-white/5 p-3 text-slate-300 transition-all duration-200 focus:border-blue-400 focus:bg-white/10"
                />
                <button
                  type="submit"
                  disabled={busy || !selectedValue}
                  className="flex items-center justify-center gap-2 rounded-lg border-2 border-blue-400 bg-blue-500/20 p-3 text-blue-200 transition-all duration-200 disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <Clock size={16} />
                  Set to {centimetres(selectedValue)} cm
                </button>
              </form>
            )}
          </section>

          <footer className="rounded-xl border border-white/10 bg-white/5 p-4 text-xs text-slate-400">
            <p className="font-mono">POST /api/command</p>
            <p className="mt-1">
              Served directly by the ESP32
              {info ? ` · v${info.version} · ${info.ip}` : ''}
            </p>
          </footer>
        </div>
      </div>
    </div>
  );
}
