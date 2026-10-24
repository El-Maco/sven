export enum SvenCommand {
  UpDuration = 'UpDuration',
  DownDuration = 'DownDuration',
  UpRelative = 'UpRelative',
  DownRelative = 'DownRelative',
  AbsoluteHeight = 'AbsoluteHeight',
  Position = 'Position',
  Calibrate = 'Calibrate',
}

export enum SvenMoveMode {
  Duration,
  Relative,
  Absolute,
  Position,
}

export enum SvenPosition {
  Bottom = 'Bottom',
  Top = 'Top',
  Armrest = 'Armrest',
  AboveArmrest = 'AboveArmrest',
  Standing = 'Standing',
  Custom = 'Custom',
}

export enum SvenDirection {
  Up,
  Down,
}

export interface DeskState {
  height_mm: number;
  position: SvenPosition | string;
  moving: boolean;
  target_mm: number | null;
  calibrated: boolean;
}

export interface SvenInfo {
  name: string;
  version: string;
  ip: string;
}

export interface SvenResponse {
  success: boolean;
  data?: unknown;
  error?: string;
  timestamp: string;
}
