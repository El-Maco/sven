export enum SvenCommand {
    UpDuration = 'UpDuration',
    DownDuration = 'DownDuration',
    UpRelative = "UpRelative",
    DownRelative = "DownRelative",
    AbsoluteHeight = "AbsoluteHeight",
    Position = "Position",
    Calibrate = "Calibrate",
}

export enum SvenMoveMode {
    Duration,
    Relative,
    Absolute,
    Position
}

export type SvenState = {
    height_mm: number;
    position: SvenPosition;
}

export enum SvenPosition {
    Bottom = "Bottom",
    Top = "Top",
    ArmRest = "ArmRest",
    AboveArmrest = "AboveArmrest",
    Standing = "Standing",
    Custom = "Custom",
}

export enum SvenDirection {
    Up,
    Down
}

export interface SvenResponse {
    success: boolean;
    /* eslint-disable  @typescript-eslint/no-explicit-any */
    data?: any;
    error?: string;
    timestamp: string;
}

