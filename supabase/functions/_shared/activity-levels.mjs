/** The reward and time curve shared by the activity-progress server functions. */
export const ACTIVITY_CURVE_VERSION = 1;

const initialStepsMinutes = [20, 30, 45, 60, 90, 120, 180, 240, 360, 720];

const rewards = [
  ["profile_frame_novice", "Рамка профиля «Новичок»"],
  ["accent_moss", "Цветовой акцент «Мох»"],
  ["badge_traveler", "Значок путешественника"],
  ["panel_pattern_stone", "Узор панели «Камень»"],
  ["scene_background_island", "Фон острова"],
  ["avatar_sprout", "Иконка профиля «Росток»"],
  ["accent_cave", "Цветовой акцент «Пещера»"],
  ["profile_frame_copper", "Рамка «Медь»"],
  ["emblem_map", "Эмблема «Карта»"],
  ["atlas_pedestal", "Атлас: постамент"],
  ["atlas_moss", "Атлас: мох"],
  ["badge_seeker", "Значок «Искатель»"],
  ["scene_background_cave", "Фон пещеры"],
  ["profile_frame_slate", "Рамка «Сланец»"],
  ["atlas_crystal", "Атлас: кристалл"],
  ["accent_amethyst", "Акцент «Аметист»"],
  ["panel_pattern_ore", "Узор панели «Руда»"],
  ["avatar_torch", "Иконка «Факел»"],
  ["atlas_lantern", "Атлас: фонарь"],
  ["profile_frame_portal", "Рамка «Портал»"],
  ["scene_background_portal", "Фон портала"],
  ["badge_explorer", "Значок «Исследователь»"],
  ["atlas_stones", "Атлас: камни"],
  ["accent_lapis", "Акцент «Лазурит»"],
  ["profile_frame_stars", "Рамка «Звёзды»"],
  ["avatar_compass", "Иконка «Компас»"],
  ["atlas_lights", "Атлас: огоньки"],
  ["panel_pattern_twilight", "Узор «Сумерки»"],
  ["badge_cartographer", "Значок «Картограф»"],
  ["atlas_mini_portal", "Атлас: мини-портал"],
  ["accent_sunset", "Акцент «Закат»"],
  ["profile_frame_crystal", "Рамка «Кристалл»"],
  ["scene_background_night", "Фон «Ночь»"],
  ["atlas_constellation", "Атлас: созвездие"],
  ["avatar_lighthouse", "Иконка «Маяк»"],
  ["panel_pattern_pollen", "Узор «Пыльца»"],
  ["badge_guardian", "Значок «Хранитель»"],
  ["atlas_flowers", "Атлас: цветы"],
  ["accent_polar", "Акцент «Полярный»"],
  ["profile_frame_atlas", "Рамка «Атлас»"],
  ["scene_background_dawn", "Фон «Рассвет»"],
  ["avatar_orb", "Иконка «Сфера»"],
  ["atlas_satellite", "Атлас: спутник"],
  ["panel_pattern_shimmer", "Узор «Сияние»"],
  ["badge_master_atlas", "Значок «Мастер атласа»"],
  ["atlas_flag", "Атлас: флаг"],
  ["accent_gold", "Акцент «Золото»"],
  ["profile_frame_constellation", "Рамка «Созвездие»"],
  ["atlas_lighthouse", "Атлас: маяк"],
  ["showcase_legend", "Витрина «Легенда» и итоговый значок"],
];

function buildCurve() {
  let totalMinutes = 0;
  return Object.freeze(
    rewards.map(([rewardId, rewardLabel], index) => {
      const level = index + 1;
      const stepMinutes =
        level <= initialStepsMinutes.length
          ? initialStepsMinutes[index]
          : 1440 + 30 * (level - 11);
      totalMinutes += stepMinutes;
      return Object.freeze({
        level,
        stepSeconds: stepMinutes * 60,
        totalSeconds: totalMinutes * 60,
        rewardId,
        rewardLabel,
      });
    }),
  );
}

export const ACTIVITY_CURVES = Object.freeze({
  [ACTIVITY_CURVE_VERSION]: buildCurve(),
});

/**
 * Calculate display progress from server-confirmed active seconds.
 * The caller must never pass a client-side timer as the source of truth.
 * @param {number} totalSeconds
 * @param {number} curveVersion
 */
export function getActivityProgress(
  totalSeconds,
  curveVersion = ACTIVITY_CURVE_VERSION,
) {
  if (!Number.isSafeInteger(totalSeconds) || totalSeconds < 0) {
    throw new RangeError("totalSeconds must be a non-negative safe integer");
  }

  const curve = ACTIVITY_CURVES[curveVersion];
  if (!curve)
    throw new RangeError(`Unknown activity curve version: ${curveVersion}`);

  let level = 0;
  for (const threshold of curve) {
    if (totalSeconds < threshold.totalSeconds) break;
    level = threshold.level;
  }

  const previous = level === 0 ? null : curve[level - 1];
  const next = curve[level] ?? null;
  const levelStartSeconds = previous?.totalSeconds ?? 0;
  const requiredSeconds = next ? next.totalSeconds - levelStartSeconds : 0;
  const secondsIntoLevel = next ? totalSeconds - levelStartSeconds : 0;

  return Object.freeze({
    curveVersion,
    level,
    totalSeconds,
    secondsIntoLevel,
    requiredSeconds,
    secondsRemaining: next ? Math.max(0, next.totalSeconds - totalSeconds) : 0,
    nextLevel: next?.level ?? null,
    unlockedRewardIds: Object.freeze(
      curve.slice(0, level).map((entry) => entry.rewardId),
    ),
  });
}
