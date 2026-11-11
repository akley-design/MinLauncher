import {
  createContext,
  useContext,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
  type PropsWithChildren,
} from "react";
import type { Session, SupabaseClient } from "@supabase/supabase-js";

export const ACTIVITY_HEARTBEAT_INTERVAL_MS = 20_000;

export type ActivityStatus =
  | "loading"
  | "signed_out"
  | "starting"
  | "active"
  | "paused"
  | "offline"
  | "unavailable";

export interface ActivityReward {
  level: number;
  stepSeconds: number;
  totalSeconds: number;
  rewardId: string;
  rewardLabel: string;
}

export interface ActivityProgress {
  curveVersion: number;
  level: number;
  totalSeconds: number;
  totalMilliseconds: number;
  confirmedSeconds: number;
  secondsIntoLevel: number;
  requiredSeconds: number;
  secondsRemaining: number;
  nextLevel: number | null;
  unlockedRewardIds: string[];
  equippedAtlasIds: string[];
  creditedMilliseconds: number;
  currentReward: ActivityReward | null;
  nextReward: ActivityReward | null;
}

export interface ActivityProgressValue {
  status: ActivityStatus;
  progress: ActivityProgress | null;
}

export interface ActivityProgressView extends ActivityProgressValue {
  userId: string | null;
}

export function formatActivityDuration(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return hours > 0 ? `${hours} ч ${minutes} мин` : `${minutes} мин`;
}

interface ActivityProgressContextValue extends ActivityProgressValue {
  authReady: boolean;
  userId: string | null;
}

interface ActivityProgressProviderProps extends PropsWithChildren {
  client: SupabaseClient | null;
  source: "site" | "launcher";
}

const EMPTY_VALUE: ActivityProgressContextValue = {
  authReady: true,
  status: "unavailable",
  progress: null,
  userId: null,
};

const ActivityProgressContext =
  createContext<ActivityProgressContextValue>(EMPTY_VALUE);
const SESSION_ID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isNonNegativeSafeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && typeof value === "number" && value >= 0;
}

function parseReward(value: unknown): ActivityReward | null | undefined {
  if (value === null) return null;
  if (!isRecord(value)) return undefined;
  if (
    !Number.isSafeInteger(value.level) ||
    typeof value.level !== "number" ||
    value.level < 1 ||
    value.level > 50 ||
    !isNonNegativeSafeInteger(value.stepSeconds) ||
    !isNonNegativeSafeInteger(value.totalSeconds) ||
    typeof value.rewardId !== "string" ||
    !/^[a-z0-9_]{1,64}$/.test(value.rewardId) ||
    typeof value.rewardLabel !== "string" ||
    value.rewardLabel.length > 120
  )
    return undefined;
  return {
    level: value.level,
    stepSeconds: value.stepSeconds,
    totalSeconds: value.totalSeconds,
    rewardId: value.rewardId,
    rewardLabel: value.rewardLabel,
  };
}

export function parseActivityProgress(value: unknown): ActivityProgress | null {
  if (!isRecord(value)) return null;
  const unlockedRewardIds = value.unlockedRewardIds;
  const equippedAtlasIds = value.equippedAtlasIds ?? [];
  if (
    !Number.isSafeInteger(value.curveVersion) ||
    typeof value.curveVersion !== "number" ||
    value.curveVersion !== 1 ||
    !Number.isSafeInteger(value.level) ||
    typeof value.level !== "number" ||
    value.level < 0 ||
    value.level > 50 ||
    !isNonNegativeSafeInteger(value.totalSeconds) ||
    !isNonNegativeSafeInteger(value.totalMilliseconds) ||
    !isNonNegativeSafeInteger(value.confirmedSeconds) ||
    value.confirmedSeconds !== value.totalSeconds ||
    Math.floor(value.totalMilliseconds / 1000) !== value.totalSeconds ||
    !isNonNegativeSafeInteger(value.secondsIntoLevel) ||
    !isNonNegativeSafeInteger(value.requiredSeconds) ||
    !isNonNegativeSafeInteger(value.secondsRemaining) ||
    !isNonNegativeSafeInteger(value.creditedMilliseconds) ||
    (value.nextLevel !== null &&
      (!Number.isSafeInteger(value.nextLevel) ||
        typeof value.nextLevel !== "number" ||
        value.nextLevel < 1 ||
        value.nextLevel > 50)) ||
    !Array.isArray(unlockedRewardIds) ||
    unlockedRewardIds.length !== value.level ||
    !unlockedRewardIds.every(
      (rewardId) =>
        typeof rewardId === "string" && /^[a-z0-9_]{1,64}$/.test(rewardId),
    ) ||
    !Array.isArray(equippedAtlasIds) ||
    equippedAtlasIds.length > 4 ||
    !equippedAtlasIds.every(
      (rewardId) =>
        typeof rewardId === "string" &&
        /^atlas_[a-z0-9_]{1,58}$/.test(rewardId) &&
        unlockedRewardIds.includes(rewardId),
    ) ||
    new Set(equippedAtlasIds).size !== equippedAtlasIds.length
  )
    return null;

  const currentReward = parseReward(value.currentReward);
  const nextReward = parseReward(value.nextReward);
  if (
    currentReward === undefined ||
    nextReward === undefined ||
    (value.level === 0 && currentReward !== null) ||
    (value.level > 0 && currentReward?.level !== value.level) ||
    (value.level > 0 &&
      currentReward?.rewardId !== unlockedRewardIds[value.level - 1]) ||
    (value.nextLevel === null && nextReward !== null) ||
    (value.nextLevel !== null && nextReward?.level !== value.nextLevel)
  )
    return null;

  return {
    curveVersion: value.curveVersion,
    level: value.level,
    totalSeconds: value.totalSeconds,
    totalMilliseconds: value.totalMilliseconds,
    confirmedSeconds: value.confirmedSeconds,
    secondsIntoLevel: value.secondsIntoLevel,
    requiredSeconds: value.requiredSeconds,
    secondsRemaining: value.secondsRemaining,
    nextLevel: value.nextLevel,
    unlockedRewardIds: [...unlockedRewardIds],
    equippedAtlasIds: [...equippedAtlasIds],
    creditedMilliseconds: value.creditedMilliseconds,
    currentReward,
    nextReward,
  };
}

function getResponseStatus(error: unknown): number | undefined {
  if (!isRecord(error) || !isRecord(error.context)) return undefined;
  return error.context.status as number | undefined;
}

function pauseLabel(): ActivityStatus {
  return navigator.onLine ? "paused" : "offline";
}

function ActivityProgressController({
  client,
  source,
  session,
  authReady,
  onValue,
}: {
  client: SupabaseClient;
  source: "site" | "launcher";
  session: Session | null;
  authReady: boolean;
  onValue: Dispatch<SetStateAction<ActivityProgressValue>>;
}) {
  useEffect(() => {
    if (!authReady) {
      onValue({ status: "loading", progress: null });
      return;
    }
    if (!session?.user.id) {
      onValue({ status: "signed_out", progress: null });
      return;
    }

    let disposed = false;
    let focusGeneration = 0;
    let sessionId: string | null = null;
    let sequence = 0;
    let starting = false;
    let heartbeatRunning = false;
    let pageHidden = false;
    let closing: Promise<void> | null = null;
    let heartbeatTimer: number | undefined;
    let retryTimer: number | undefined;
    let nativeFocused = document.hasFocus();

    const setValue = (
      status: ActivityStatus,
      progress?: ActivityProgress | null,
    ) => {
      if (disposed) return;
      onValue((previous) => ({
        status,
        progress: progress === undefined ? previous.progress : progress,
      }));
    };
    const isPageActive = () =>
      !pageHidden &&
      navigator.onLine &&
      document.visibilityState === "visible" &&
      (source === "site" ? document.hasFocus() : nativeFocused);
    const clearHeartbeat = () => {
      if (heartbeatTimer !== undefined) {
        window.clearInterval(heartbeatTimer);
        heartbeatTimer = undefined;
      }
    };
    const clearRetry = () => {
      if (retryTimer !== undefined) {
        window.clearTimeout(retryTimer);
        retryTimer = undefined;
      }
    };
    const closeRemoteSession = async (id: string) => {
      const { error } = await client.functions.invoke("activity-progress", {
        body: { action: "pause", session_id: id },
      });
      if (error) throw error;
    };

    const pauseSession = async () => {
      clearHeartbeat();
      clearRetry();
      const id = sessionId;
      sessionId = null;
      sequence = 0;
      setValue(pauseLabel());
      if (!id) return;
      if (!closing) {
        closing = closeRemoteSession(id).catch(() => {
          if (!disposed) setValue("offline");
        });
      }
      try {
        await closing;
      } finally {
        closing = null;
      }
      if (!disposed && isPageActive()) void ensureSession();
    };

    const heartbeat = async () => {
      if (disposed || heartbeatRunning || !sessionId) return;
      if (!isPageActive()) {
        await pauseSession();
        return;
      }

      heartbeatRunning = true;
      const heartbeatSessionId = sessionId;
      const nextSequence = sequence + 1;
      try {
        const { data, error } = await client.functions.invoke(
          "activity-progress",
          {
            body: {
              action: "heartbeat",
              session_id: heartbeatSessionId,
              seq: nextSequence,
            },
          },
        );
        if (disposed || sessionId !== heartbeatSessionId) return;
        if (error) {
          const status = getResponseStatus(error);
          if (status === 404 || status === 409) {
            sessionId = null;
            sequence = 0;
            clearHeartbeat();
            setValue("offline");
            if (isPageActive()) void ensureSession();
          } else {
            setValue("offline");
          }
          return;
        }
        const progress = parseActivityProgress(data);
        if (!progress) {
          setValue("offline");
          return;
        }
        if (!isPageActive()) {
          await pauseSession();
          return;
        }
        sequence = nextSequence;
        setValue("active", progress);
      } catch {
        if (!disposed) setValue("offline");
      } finally {
        heartbeatRunning = false;
      }
    };

    async function ensureSession() {
      if (disposed) return;
      if (!isPageActive()) {
        setValue(pauseLabel());
        return;
      }
      if (sessionId || starting) return;
      if (closing) {
        void closing.then(() => ensureSession());
        return;
      }

      starting = true;
      clearRetry();
      const requestGeneration = focusGeneration;
      setValue("starting");
      try {
        const { data, error } = await client.functions.invoke(
          "activity-progress",
          {
            body: { action: "start", source },
          },
        );
        if (error) throw error;
        if (
          !isRecord(data) ||
          typeof data.sessionId !== "string" ||
          !SESSION_ID_PATTERN.test(data.sessionId)
        ) {
          throw new Error("Invalid activity session response");
        }
        if (disposed) {
          await closeRemoteSession(data.sessionId);
          return;
        }
        if (requestGeneration !== focusGeneration) {
          await closeRemoteSession(data.sessionId);
          if (isPageActive()) void ensureSession();
          return;
        }
        sessionId = data.sessionId;
        sequence = 0;
        if (!isPageActive()) {
          await pauseSession();
          return;
        }
        setValue("active");
        clearRetry();
        clearHeartbeat();
        heartbeatTimer = window.setInterval(
          () => void heartbeat(),
          ACTIVITY_HEARTBEAT_INTERVAL_MS,
        );
      } catch {
        if (!disposed) {
          setValue("offline");
          clearRetry();
          retryTimer = window.setTimeout(
            () => void ensureSession(),
            ACTIVITY_HEARTBEAT_INTERVAL_MS,
          );
        }
      } finally {
        starting = false;
        if (
          !disposed &&
          isPageActive() &&
          !sessionId &&
          retryTimer === undefined
        ) {
          void ensureSession();
        }
      }
    }

    async function refreshProgress() {
      try {
        const { data, error } = await client.functions.invoke(
          "activity-progress",
          {
            body: { action: "progress" },
          },
        );
        if (disposed) return;
        if (error) {
          setValue("offline", null);
          return;
        }
        const progress = parseActivityProgress(data);
        if (!progress) {
          setValue("offline", null);
          return;
        }
        setValue(
          isPageActive() ? (sessionId ? "active" : "starting") : pauseLabel(),
          progress,
        );
      } catch {
        if (!disposed) setValue("offline", null);
      }
    }

    const reconcile = () => {
      if (isPageActive()) void ensureSession();
      else {
        focusGeneration += 1;
        void pauseSession();
      }
    };
    const onNativeFocus = (event: Event) => {
      if ("detail" in event && typeof event.detail === "boolean") {
        nativeFocused = event.detail;
      }
      reconcile();
    };
    const onOffline = () => {
      setValue("offline");
      focusGeneration += 1;
      void pauseSession();
    };
    const onPageHide = () => {
      pageHidden = true;
      focusGeneration += 1;
      void pauseSession();
    };
    const onPageShow = () => {
      pageHidden = false;
      reconcile();
    };

    setValue("loading", null);
    void refreshProgress();
    reconcile();
    document.addEventListener("visibilitychange", reconcile);
    window.addEventListener("focus", reconcile);
    window.addEventListener("blur", reconcile);
    window.addEventListener("pagehide", onPageHide);
    window.addEventListener("pageshow", onPageShow);
    window.addEventListener("online", reconcile);
    window.addEventListener("offline", onOffline);
    if (source === "launcher") {
      window.addEventListener("minlauncher:window-focus", onNativeFocus);
    }

    return () => {
      disposed = true;
      clearHeartbeat();
      clearRetry();
      document.removeEventListener("visibilitychange", reconcile);
      window.removeEventListener("focus", reconcile);
      window.removeEventListener("blur", reconcile);
      window.removeEventListener("pagehide", onPageHide);
      window.removeEventListener("pageshow", onPageShow);
      window.removeEventListener("online", reconcile);
      window.removeEventListener("offline", onOffline);
      if (source === "launcher") {
        window.removeEventListener("minlauncher:window-focus", onNativeFocus);
      }
      if (sessionId) {
        void closeRemoteSession(sessionId).catch(() => {
          console.warn(
            "Не удалось закрыть серверную сессию активности при размонтировании.",
          );
        });
        sessionId = null;
      }
    };
  }, [authReady, client, onValue, session?.user.id, source]);

  return null;
}

export function ActivityProgressProvider({
  children,
  client,
  source,
}: ActivityProgressProviderProps) {
  const [session, setSession] = useState<Session | null>(null);
  const [authReady, setAuthReady] = useState(!client);
  const [value, setValue] = useState<ActivityProgressValue>({
    status: client ? "loading" : "unavailable",
    progress: null,
  });

  useEffect(() => {
    if (!client) {
      setSession(null);
      setAuthReady(true);
      setValue({ status: "unavailable", progress: null });
      return;
    }

    let active = true;
    let authEventSeen = false;
    setAuthReady(false);
    const {
      data: { subscription },
    } = client.auth.onAuthStateChange((_event, nextSession) => {
      authEventSeen = true;
      if (!active) return;
      setSession(nextSession);
      setAuthReady(true);
    });
    void client.auth
      .getSession()
      .then(({ data, error }) => {
        if (!active || authEventSeen) return;
        setSession(data.session);
        setAuthReady(true);
        if (error) setValue({ status: "offline", progress: null });
      })
      .catch(() => {
        if (!active || authEventSeen) return;
        setSession(null);
        setAuthReady(true);
        setValue({ status: "offline", progress: null });
      });

    return () => {
      active = false;
      subscription.unsubscribe();
    };
  }, [client]);

  return (
    <ActivityProgressContext.Provider
      value={{ ...value, authReady, userId: session?.user.id ?? null }}
    >
      {client ? (
        <ActivityProgressController
          client={client}
          source={source}
          session={session}
          authReady={authReady}
          onValue={setValue}
        />
      ) : null}
      {children}
    </ActivityProgressContext.Provider>
  );
}

export function useActivityProgress(): ActivityProgressView {
  const { status, progress, userId } = useContext(ActivityProgressContext);
  return { status, progress, userId };
}

const STATUS_TEXT: Record<ActivityStatus, string> = {
  loading: "Загружаем сохранённый прогресс…",
  signed_out: "Войдите в аккаунт, чтобы сохранять уровень.",
  starting: "Проверяем активную сессию…",
  active: "Зачёт идёт, пока это окно открыто и в фокусе.",
  paused: "Пауза. Вернитесь в это окно, чтобы продолжить.",
  offline: "Нет связи с сервером. Время сейчас не начисляется.",
  unavailable: "Прогресс сейчас недоступен.",
};

const COMPACT_STATUS_TEXT: Record<ActivityStatus, string> = {
  loading: "Загружаем уровень",
  signed_out: "Войдите, чтобы копить",
  starting: "Запускаем зачёт",
  active: "Время начисляется",
  paused: "Пауза: вернитесь сюда",
  offline: "Нет связи, зачёт остановлен",
  unavailable: "Прогресс недоступен",
};

export function ActivityProgressSummary({
  compact = false,
}: { compact?: boolean } = {}) {
  const { status, progress } = useActivityProgress();
  const hasProgress = progress !== null;
  const level = hasProgress ? progress.level : null;
  const atMaximum = level === 50;
  const progressMaximum = progress?.requiredSeconds || 1;
  const progressValue = atMaximum
    ? 1
    : Math.min(progressMaximum, progress?.secondsIntoLevel ?? 0);
  const percent = Math.round((progressValue / progressMaximum) * 100);

  return (
    <section
      className={
        compact
          ? "activity-progress activity-progress--compact"
          : "activity-progress"
      }
      data-state={status}
      aria-label="Прогресс Атласа миров"
    >
      <div className="activity-progress__head">
        <div>
          <span className="activity-progress__eyebrow">АТЛАС МИРОВ</span>
          <strong>
            {level === null ? "—" : level} <span>/ 50</span>
          </strong>
        </div>
        <span className="activity-progress__time">
          {hasProgress
            ? formatActivityDuration(progress.confirmedSeconds)
            : "—"}
        </span>
      </div>
      <div
        className="activity-progress__track"
        role="progressbar"
        aria-label="Прогресс до следующего уровня"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent}
        aria-valuetext={
          atMaximum
            ? "Максимальный уровень"
            : `${percent}% до следующего уровня`
        }
      >
        <span style={{ width: `${percent}%` }} />
      </div>
      <p
        className="activity-progress__status"
        role={compact ? undefined : "status"}
        aria-live={compact ? undefined : "polite"}
      >
        {compact ? COMPACT_STATUS_TEXT[status] : STATUS_TEXT[status]}
      </p>
      {hasProgress ? (
        <>
          {!compact && progress.currentReward ? (
            <p className="activity-progress__reward">
              Открыто: {progress.currentReward.rewardLabel}
            </p>
          ) : null}
          <p className="activity-progress__reward">
            {atMaximum
              ? "Все награды Атласа открыты."
              : progress.nextReward
                ? `Дальше: ${progress.nextReward.rewardLabel} · ${formatActivityDuration(progress.secondsRemaining)}`
                : "Начните открывать страницы Атласа."}
          </p>
        </>
      ) : null}
    </section>
  );
}
