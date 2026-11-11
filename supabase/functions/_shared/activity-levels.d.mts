export interface ActivityReward {
    readonly level: number;
    readonly stepSeconds: number;
    readonly totalSeconds: number;
    readonly rewardId: string;
    readonly rewardLabel: string;
}

export const ACTIVITY_CURVE_VERSION: 1;
export const ACTIVITY_CURVES: Readonly<{
    1: readonly ActivityReward[];
}>;

export function getActivityProgress(
    totalSeconds: number,
    curveVersion?: number,
): Readonly<{
    curveVersion: number;
    level: number;
    totalSeconds: number;
    secondsIntoLevel: number;
    requiredSeconds: number;
    secondsRemaining: number;
    nextLevel: number | null;
    unlockedRewardIds: readonly string[];
}>;
