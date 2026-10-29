import {
  Fragment,
  lazy,
  Suspense,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { Session } from "@supabase/supabase-js";
import { articles, type NewsArticle } from "./news";
import {
  PageMeta,
  resolvePage,
  type PageAccent,
  type PageId,
  type PageMetaInfo,
} from "./page-meta";
import {
  getReleaseClock,
  getWindowsRelease,
  ReleaseUnavailable,
  type WindowsRelease,
} from "./downloads";
import { BrandMark } from "@minlauncher/visuals/brand";
import { SiteDiorama } from "@minlauncher/visuals/site";
import { useAccountSession } from "./account-session";
import { ResponsiveImage, siteMedia } from "./media";
import { Icon, PrimaryLink } from "./ui";

const AuthPage = lazy(() =>
  import("./account-routes").then((module) => ({ default: module.AuthPage })),
);
const AccountPage = lazy(() =>
  import("./account-routes").then((module) => ({
    default: module.AccountPage,
  })),
);
const PasswordRecoveryPage = lazy(() =>
  import("./account-routes").then((module) => ({
    default: module.PasswordRecoveryPage,
  })),
);
const CallbackPage = lazy(() =>
  import("./account-routes").then((module) => ({
    default: module.CallbackPage,
  })),
);

function ArticleCard({
  article,
  index = 0,
  variant = "default",
}: {
  article: NewsArticle;
  index?: number;
  variant?: "default" | "feature" | "row";
}) {
  const cover = siteMedia.cover[article.scene];
  if (variant === "row")
    return (
      <a
        className={`article-row article-row--${article.scene}`}
        href={`/news/${article.slug}`}
      >
        <span className="article-row__index">
          {String(index + 1).padStart(2, "0")}
        </span>
        <div className="article-row__copy">
          <span className="eyebrow">{article.category}</span>
          <h3>{article.title}</h3>
          <p>{article.excerpt}</p>
        </div>
        <div className="article-row__thumb">
          <ResponsiveImage image={cover} alt="" />
          <span>
            {article.readTime} <Icon name="arrow" />
          </span>
        </div>
      </a>
    );
  return (
    <a
      className={`article-card article-card--${variant} article-card--scene-${article.scene}`}
      href={`/news/${article.slug}`}
    >
      <div className="article-card__visual">
        <ResponsiveImage
          image={cover}
          alt=""
          priority={variant === "feature"}
        />
        <span>
          {article.readTime} <Icon name="arrow" />
        </span>
      </div>
      <div className="article-card__body">
        <span className="eyebrow">{article.category}</span>
        <h3>{article.title}</h3>
        <p>{article.excerpt}</p>
      </div>
    </a>
  );
}

function Header({
  session,
  pathname,
}: {
  session: Session | null;
  pathname: string;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const nav = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!menuOpen) return;
    nav.current?.querySelector<HTMLElement>("a")?.focus();
    document.documentElement.classList.add("nav-open");
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setMenuOpen(false);
        toggle.current?.focus();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = [
        ...(nav.current?.querySelectorAll<HTMLElement>("a[href]") ?? []),
        toggle.current,
      ].filter((element): element is HTMLElement => Boolean(element));
      if (focusable.length === 0) return;
      const first = focusable[0]!;
      const last = focusable[focusable.length - 1]!;
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", keydown);
    return () => {
      window.removeEventListener("keydown", keydown);
      document.documentElement.classList.remove("nav-open");
    };
  }, [menuOpen]);

  const links = [
    { href: "/features", label: "Возможности" },
    { href: "/download", label: "Скачать" },
    { href: "/news", label: "Журнал" },
  ];
  const active = (href: string) =>
    pathname === href || pathname.startsWith(`${href}/`);
  const accountActive =
    pathname === "/account" ||
    pathname.startsWith("/account/") ||
    pathname.startsWith("/auth/");
  return (
    <header className="site-header">
      <a
        className="brand"
        href="/"
        aria-label="MinLauncher — главная"
        aria-current={pathname === "/" ? "page" : undefined}
      >
        <BrandMark />
        <span>
          minlauncher<span className="brand-dot">.</span>
        </span>
      </a>
      <nav
        ref={nav}
        id="site-navigation"
        className={menuOpen ? "site-nav site-nav--open" : "site-nav"}
        aria-label="Основная навигация"
      >
        {links.map((link) => (
          <a
            key={link.href}
            aria-current={active(link.href) ? "page" : undefined}
            href={link.href}
            onClick={() => setMenuOpen(false)}
          >
            {link.label}
          </a>
        ))}
        <a
          className="nav-account"
          aria-current={accountActive ? "page" : undefined}
          href={session ? "/account/settings" : "/account"}
          onClick={() => setMenuOpen(false)}
        >
          {session ? "Мой профиль" : "Войти"}
          <Icon name="arrow" />
        </a>
      </nav>
      {menuOpen && (
        <div
          className="site-nav-scrim"
          aria-hidden="true"
          onClick={() => setMenuOpen(false)}
        />
      )}
      <button
        ref={toggle}
        className="menu-toggle"
        type="button"
        aria-controls="site-navigation"
        aria-label={menuOpen ? "Закрыть меню" : "Открыть меню"}
        aria-expanded={menuOpen}
        onClick={() => setMenuOpen((open) => !open)}
      >
        <Icon name={menuOpen ? "close" : "menu"} />
      </button>
    </header>
  );
}

function Footer() {
  return (
    <footer className="site-footer">
      <div>
        <a className="brand" href="/">
          <BrandMark />
          <span>minlauncher.</span>
        </a>
        <p>Лаунчер для Minecraft Java.</p>
      </div>
      <div className="footer-links">
        <a href="/features">Возможности</a>
        <a href="/download">Скачать</a>
        <a href="/news">Журнал</a>
        <a
          href="https://github.com/akley-design/MinLauncher"
          target="_blank"
          rel="noreferrer"
        >
          GitHub ↗
        </a>
      </div>
      <div className="footer-bottom">
        <span>© MinLauncher</span>
        <span>Независимый проект. Не связан с Mojang или Microsoft.</span>
      </div>
    </footer>
  );
}

function PageFrame({
  children,
  session,
  pathname,
  page,
  accent,
  meta,
}: {
  children: ReactNode;
  session: Session | null;
  pathname: string;
  page: PageId;
  accent: PageAccent;
  meta: PageMetaInfo;
}) {
  return (
    <div className="site" data-page={page} data-accent={accent}>
      <PageMeta meta={meta} />
      <a className="skip-link" href="#main-content">
        К содержимому
      </a>
      <Header session={session} pathname={pathname} />
      <main id="main-content" tabIndex={-1}>
        {children}
      </main>
      <Footer />
    </div>
  );
}

function HomePage() {
  return (
    <>
      <section className="hero section-wrap">
        <div className="hero__content">
          <span className="eyebrow">
            <i /> Лаунчер для Minecraft Java
          </span>
          <h1>
            Твой
            <br />
            <em>Minecraft</em>
            <span className="title-dot">.</span>
          </h1>
          <p className="hero__summary">
            Выбирай версию Minecraft, создавай сборки
            <br />и сохраняй настройки для каждой из них.
          </p>
          <div className="hero__actions">
            <PrimaryLink href="/download">Скачать для Windows</PrimaryLink>
            <a className="text-link" href="#launcher">
              Посмотреть интерфейс <span>↓</span>
            </a>
          </div>
          <span className="hero__caption">Настрой Minecraft под себя</span>
        </div>
        <div className="hero__world">
          <SiteDiorama
            variant="island"
            poster={siteMedia.poster.island}
            priority
          />
          <div className="world-caption">
            <span>Вот идея что можно построить!</span>
            <span>Minecraft Java</span>
          </div>
        </div>
      </section>
      <section className="launcher-section section-wrap" id="launcher">
        <div className="section-heading">
          <span className="eyebrow">Интерфейс лаунчера</span>
          <h2>
            Версии
            <br />
            <em>и сборки.</em>
          </h2>
          <p>
            Выбирай версию, создавай отдельные игровые профили и меняй настройки
            перед запуском.
          </p>
        </div>
        <figure className="launcher-shot">
          <div className="window-chrome">
            <span>
              <i />
              <i />
              <i />
            </span>
            <b>minlauncher / играть</b>
          </div>
          <ResponsiveImage
            image={siteMedia.launcherPreview}
            alt="Интерфейс MinLauncher: выбор версии Minecraft и запуск игры"
            lazy
          />{" "}
          <figcaption>Игра, версии и настройки — в одном окне.</figcaption>
        </figure>
      </section>
      <section className="feature-story section-wrap">
        <div className="story-art">
          <SiteDiorama variant="cave" poster={siteMedia.poster.cave} lazy />
          <span className="world-caption">02 / Пещера</span>
        </div>
        <div className="story-copy">
          <span className="eyebrow">Игровые профили</span>
          <h2>
            Настройки
            <br />
            <em>для сборки.</em>
          </h2>
          <div className="feature-step">
            <span>01</span>
            <div>
              <h3>Отдельный профиль для каждой версии</h3>
              <p>
                Каждая сборка хранит свои файлы и настройки в отдельной папке.
              </p>
            </div>
          </div>
          <div className="feature-step">
            <span>02</span>
            <div>
              <h3>Настройки перед стартом</h3>
              <p>Управляй Java, памятью и параметрами игры из лаунчера.</p>
            </div>
          </div>
          <div className="feature-step">
            <span>03</span>
            <div>
              <h3>Повторное использование файлов</h3>
              <p>Уже загруженные библиотеки и ресурсы используются повторно.</p>
            </div>
          </div>
          <a className="text-link" href="/features">
            Все возможности <Icon name="arrow" />
          </a>
        </div>
      </section>
      <section className="journal section-wrap">
        <div className="journal-heading">
          <div>
            <span className="eyebrow">Из журнала</span>
            <h2>
              Журнал
              <br />
              <em>MinLauncher.</em>
            </h2>
          </div>
          <a className="text-link" href="/news">
            Все заметки <Icon name="arrow" />
          </a>
        </div>
        <div className="article-grid">
          {articles.slice(0, 2).map((article, index) => (
            <ArticleCard key={article.slug} article={article} index={index} />
          ))}
        </div>
      </section>
      <section className="final-cta section-wrap">
        <span className="eyebrow">Для Windows x64</span>
        <h2>
          Скачать
          <br />
          <em>MinLauncher.</em>
        </h2>
        <PrimaryLink href="/download">Скачать MinLauncher</PrimaryLink>
      </section>
    </>
  );
}

function FeaturesPage() {
  const features = [
    {
      title: "Отдельные игровые папки",
      text: "Создавай отдельные инстансы Minecraft со своими файлами, именем и настройками.",
      tag: "ПРОФИЛИ",
    },
    {
      title: "Java и память",
      text: "Выбирай Java и объём памяти. Сохраняй настройки, чтобы возвращаться к привычной игре.",
      tag: "RUNTIME",
    },
    {
      title: "Загрузка библиотек и ресурсов",
      text: "Лаунчер использует уже скачанные библиотеки и ресурсы, когда они подходят выбранной версии.",
      tag: "CACHE",
    },
    {
      title: "Замеры запуска",
      text: "Измеряй этапы запуска и сравнивай результаты с учётом компьютера и условий теста.",
      tag: "BENCH",
    },
  ];
  return (
    <section className="page features-page">
      <header className="feature-hero section-wrap">
        <div className="feature-hero__copy">
          <div className="page-kicker">
            <span className="eyebrow">
              <i /> Полевой справочник
            </span>
            <span className="page-index">01 / Возможности</span>
          </div>
          <h1 className="page-display">
            Что внутри
            <br />
            <em>лаунчера.</em>
          </h1>
          <p className="page-lede">
            Версии, изолированные профили, Java, память и замеры запуска — без
            лишних папок и ручной сборки аргументов.
          </p>
          <dl className="feature-stats">
            <div>
              <dt>04</dt>
              <dd>направления</dd>
            </div>
            <div>
              <dt>x64</dt>
              <dd>Windows</dd>
            </div>
            <div>
              <dt>0.1.1</dt>
              <dd>beta</dd>
            </div>
          </dl>
        </div>
        <div className="feature-hero__art page-art">
          <ResponsiveImage
            image={siteMedia.wide.cave}
            alt="Пещера в стиле Minecraft — раздел возможностей"
            priority
          />
          <span className="art-caption">CAVE / FIELD GUIDE</span>
        </div>
      </header>
      <div className="feature-board section-wrap">
        <div className="feature-stage">
          <SiteDiorama variant="cave" poster={siteMedia.poster.cave} lazy />
          <div className="stage-caption">
            <span>Экспонат</span>
            <span>Пещера / 01</span>
          </div>
        </div>
        <div className="feature-notes">
          {features.map((item, index) => (
            <article className="feature-note" key={item.title}>
              <span className="feature-note__num">0{index + 1}</span>
              <div>
                <h2>{item.title}</h2>
                <p>{item.text}</p>
              </div>
              <span className="feature-note__tag">{item.tag}</span>
            </article>
          ))}
        </div>
      </div>
      <div className="page-cta section-wrap">
        <span>Осталось выбрать версию и нажать «Играть».</span>
        <PrimaryLink href="/download">Скачать лаунчер</PrimaryLink>
      </div>
    </section>
  );
}

type DownloadState =
  | { kind: "loading" }
  | { kind: "ready"; release: WindowsRelease }
  | {
      kind: "scheduled";
      release: WindowsRelease;
      releaseAt: number;
      serverOffset: number;
    }
  | { kind: "unpublished" }
  | { kind: "error" };

function formatReleaseCountdown(milliseconds: number) {
  const value = Math.max(0, milliseconds);
  const days = Math.floor(value / 86_400_000);
  const hours = Math.floor((value % 86_400_000) / 3_600_000);
  const minutes = Math.floor((value % 3_600_000) / 60_000);
  const seconds = Math.floor((value % 60_000) / 1_000);
  const millis = Math.floor(value % 1_000);
  return `${days ? `${days}д ` : ""}${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}

function DownloadPage() {
  const [state, setState] = useState<DownloadState>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [serverNow, setServerNow] = useState(Date.now());
  useEffect(() => {
    const controller = new AbortController();
    setState({ kind: "loading" });
    void Promise.all([
      getWindowsRelease(controller.signal),
      getReleaseClock(controller.signal),
    ])
      .then(([release, clock]) => {
        if (controller.signal.aborted) return;
        const releaseAt = release.publishedAt
          ? Date.parse(release.publishedAt)
          : clock.releaseAt;
        const serverOffset = clock.now - Date.now();
        if (clock.now < releaseAt) {
          setServerNow(clock.now);
          setState({ kind: "scheduled", release, releaseAt, serverOffset });
        } else {
          setState({ kind: "ready", release });
        }
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted)
          setState({
            kind: error instanceof ReleaseUnavailable ? "unpublished" : "error",
          });
      });
    return () => controller.abort();
  }, [attempt]);
  useEffect(() => {
    if (state.kind !== "scheduled") return;
    const update = () => {
      const current = Date.now() + state.serverOffset;
      setServerNow(current);
      if (current >= state.releaseAt)
        setState({ kind: "ready", release: state.release });
    };
    update();
    const timer = window.setInterval(update, 250);
    return () => window.clearInterval(timer);
  }, [state]);
  const status =
    state.kind === "loading"
      ? "Проверяем последний релиз…"
      : state.kind === "unpublished"
        ? "Установщик ещё не опубликован"
        : state.kind === "error"
          ? "Не удалось проверить релиз"
          : state.kind === "scheduled"
            ? "Бета-версия скоро стартует · Windows x64"
            : state.release.beta
              ? "Бета-версия · Windows x64"
              : "Последний опубликованный релиз";
  return (
    <section className="page download-bay">
      <header className="download-bay__intro section-wrap">
        <div className="page-kicker">
          <span className="eyebrow">
            <i /> MinLauncher / Загрузка
          </span>
          <span className="page-index">03 / Windows x64</span>
        </div>
        <h1 className="product-heading page-display">
          Скачать
          <br />
          <em>MinLauncher.</em>
        </h1>
        <p className="download-intro page-lede">
          Установи лаунчер, выбери версию Minecraft и настрой игровой профиль.
        </p>
      </header>
      <div className="download-grid section-wrap">
        <div className="release-console">
          <div className="release-console__head">
            <BrandMark />
            <div>
              <h2>MinLauncher</h2>
              <span>Windows · x64</span>
            </div>
          </div>
          <p
            className={`release-status release-status--${state.kind}`}
            role="status"
          >
            {status}
          </p>
          {state.kind === "ready" ? (
            <>
              <a className="button button--primary" href={state.release.url}>
                {state.release.beta
                  ? "Скачать бета-версию"
                  : "Скачать для Windows"}{" "}
                <Icon name="arrow" />
              </a>
              {state.release.beta && (
                <p className="download-beta-note">
                  Бета-сборка для тестирования. Возможны ошибки и неполадки.
                </p>
              )}
              <dl className="release-meta">
                {state.release.version && (
                  <div>
                    <dt>Версия</dt>
                    <dd>{state.release.version}</dd>
                  </div>
                )}
                {state.release.publishedAt && (
                  <div>
                    <dt>Опубликовано</dt>
                    <dd>
                      {new Intl.DateTimeFormat("ru-RU", {
                        day: "numeric",
                        month: "long",
                        year: "numeric",
                      }).format(new Date(state.release.publishedAt))}
                    </dd>
                  </div>
                )}
              </dl>
            </>
          ) : state.kind === "scheduled" ? (
            <>
              <p className="download-countdown-label">
                Старт бета-версии через
              </p>
              <p
                className="download-countdown"
                role="timer"
                aria-label="Осталось до запуска бета-версии"
              >
                {formatReleaseCountdown(state.releaseAt - serverNow)}
              </p>
              <button className="button button--primary" disabled type="button">
                Бета откроется автоматически
              </button>
            </>
          ) : (
            <>
              <button className="button button--primary" disabled type="button">
                {state.kind === "loading"
                  ? "Проверяем загрузку…"
                  : "Загрузка пока недоступна"}
              </button>
              {state.kind === "error" && (
                <>
                  <p>Проверь соединение и попробуй ещё раз.</p>
                  <button
                    className="text-link"
                    type="button"
                    onClick={() => setAttempt((value) => value + 1)}
                  >
                    Повторить проверку <Icon name="arrow" />
                  </button>
                </>
              )}
              {state.kind === "unpublished" && (
                <p>
                  Загляни сюда позже — ссылка появится вместе с первым релизом.
                </p>
              )}
            </>
          )}
          <div className="download-steps">
            <div className="download-step">
              <b>01</b>
              <span>Проверяем последний релиз и версию.</span>
            </div>
            <div className="download-step">
              <b>02</b>
              <span>Скачиваешь установщик Windows x64.</span>
            </div>
            <div className="download-step">
              <b>03</b>
              <span>Настраиваешь профиль и запускаешь игру.</span>
            </div>
          </div>
          <span className="download-footnote">
            {state.kind === "ready" && state.release.beta
              ? "MinLauncher 0.1.1-beta · Windows x64"
              : "Скачивание с GitHub Releases"}
          </span>
        </div>
        <div className="download-stage page-art">
          <ResponsiveImage
            image={siteMedia.wide.portal}
            alt=""
            imgClassName="download-stage__wide"
            priority
          />
          <div className="download-stage__scene">
            <SiteDiorama
              variant="portal"
              poster={siteMedia.poster.portal}
              priority
            />
          </div>
          <div className="stage-caption">
            <span>Портал / Windows</span>
            <span>03 / Download</span>
          </div>
        </div>
      </div>
      <div className="download-notes section-wrap">
        <article>
          <span className="eyebrow">Установщик</span>
          <h3>Windows x64</h3>
          <p>
            Один установщик для Windows. Ссылка ведёт на последний
            опубликованный релиз.
          </p>
        </article>
        <article>
          <span className="eyebrow">Проверка</span>
          <h3>Версия и дата</h3>
          <p>
            Перед скачиванием показываем версию и дату публикации из манифеста
            релиза.
          </p>
        </article>
        <article>
          <span className="eyebrow">После установки</span>
          <h3>Свой профиль</h3>
          <p>
            Выбери версию, настрой Java и память — лаунчер сохранит параметры
            профиля.
          </p>
        </article>
      </div>
    </section>
  );
}

function NewsPage() {
  const featured = articles[0]!;
  const notes = articles.slice(1);
  return (
    <section className="page news-page">
      <header className="news-masthead section-wrap">
        <div className="page-kicker">
          <span className="eyebrow">
            <i /> Журнал
          </span>
          <span className="page-index">02 / Заметки</span>
        </div>
        <div className="news-masthead__grid">
          <h1 className="page-display">
            Заметки
            <br />
            <em>и наблюдения.</em>
          </h1>
          <div className="news-issue">
            <span>Выпуск</span>
            <strong>{String(articles.length).padStart(2, "0")}</strong>
            <span>записи · MinLauncher</span>
          </div>
        </div>
        <p className="page-lede">
          О релизах, производительности и деталях, которые обычно остаются за
          кадром.
        </p>
      </header>
      <div className="news-feature section-wrap">
        <ArticleCard article={featured} variant="feature" />
      </div>
      <section className="news-world section-wrap" aria-label="Наблюдение">
        <div className="news-world__art" aria-hidden="true">
          <ResponsiveImage image={siteMedia.wide.cave} alt="" lazy />
        </div>
        <blockquote className="news-world__quote">
          <span className="eyebrow">Наблюдение</span>
          <p>
            Под землёй видно то, что обычно скрыто: слои породы, рельсы и свет,
            который ведёт наружу.
          </p>
          <cite>Cave / 02</cite>
        </blockquote>
      </section>
      <div className="news-contents section-wrap">
        <div className="news-contents__head">
          <span className="eyebrow">Содержание</span>
          <span>{String(notes.length).padStart(2, "0")} заметки</span>
        </div>
        {notes.map((article, index) => (
          <ArticleCard
            key={article.slug}
            article={article}
            index={index + 1}
            variant="row"
          />
        ))}
      </div>
    </section>
  );
}

function formatArticleDate(value: string) {
  const date = new Date(value);
  const months = [
    "января",
    "февраля",
    "марта",
    "апреля",
    "мая",
    "июня",
    "июля",
    "августа",
    "сентября",
    "октября",
    "ноября",
    "декабря",
  ];
  return `${date.getUTCDate()} ${months[date.getUTCMonth()]} ${date.getUTCFullYear()}`;
}

function ArticlePage({ slug }: { slug: string }) {
  const article = articles.find((item) => item.slug === slug);
  if (!article) return <NotFoundPage />;
  const wide = siteMedia.wide[article.scene];
  const related = articles.filter((item) => item.slug !== slug);
  return (
    <article className={`page article-page article-page--${article.scene}`}>
      <header className="article-hero section-wrap">
        <a className="back-link" href="/news">
          ← Журнал
        </a>
        <div className="page-kicker">
          <span className="eyebrow">
            <i /> {article.category}
          </span>
          <span className="page-index">{article.readTime} / Журнал</span>
        </div>
        <h1 className="article-hero__title">{article.title}</h1>
        <p className="article-hero__excerpt">{article.excerpt}</p>
      </header>
      <div className="article-hero__art section-wrap">
        <ResponsiveImage
          image={wide}
          alt={`Иллюстрация к статье «${article.title}»`}
          priority
        />
      </div>
      <div className="article-layout section-wrap">
        <aside className="article-rail">
          <span className="eyebrow">Полевая заметка</span>
          <dl>
            <div>
              <dt>Категория</dt>
              <dd>{article.category}</dd>
            </div>
            <div>
              <dt>Чтение</dt>
              <dd>{article.readTime}</dd>
            </div>
            <div>
              <dt>Опубликовано</dt>
              <dd>
                <time dateTime={article.publishedAt}>
                  {formatArticleDate(article.publishedAt)}
                </time>
              </dd>
            </div>
            <div>
              <dt>Автор</dt>
              <dd>{article.author}</dd>
            </div>
            <div>
              <dt>Журнал</dt>
              <dd>MinLauncher</dd>
            </div>
          </dl>
        </aside>
        <div className="article-body">
          {article.body.map((paragraph, index) => (
            <Fragment key={paragraph}>
              {index > 0 && article.headings[index - 1] ? (
                <h2>{article.headings[index - 1]}</h2>
              ) : null}
              <p>{paragraph}</p>
            </Fragment>
          ))}
          <PrimaryLink href="/download" variant="secondary">
            Попробовать MinLauncher
          </PrimaryLink>
        </div>
      </div>
      <aside className="article-more section-wrap">
        <h2>Ещё из журнала</h2>
        <div className="article-more__grid">
          {related.map((item, index) => (
            <ArticleCard
              key={item.slug}
              article={item}
              index={index}
              variant="row"
            />
          ))}
        </div>
      </aside>
    </article>
  );
}

function NotFoundPage() {
  return (
    <section className="page not-found">
      <div className="not-found__art section-wrap" aria-hidden="true">
        <ResponsiveImage image={siteMedia.wide.cave} alt="" lazy />
        <span className="not-found__number">404</span>
      </div>
      <div className="not-found__copy section-wrap">
        <div className="page-kicker">
          <span className="eyebrow">
            <i /> Ошибка маршрута
          </span>
          <span className="page-index">404 / MinLauncher</span>
        </div>
        <h1 className="page-display">
          Страница
          <br />
          <em>потерялась.</em>
        </h1>
        <p className="page-lede">
          Такого адреса нет. Вернись на главную или забери лаунчер — там всё на
          месте.
        </p>
        <div className="not-found__actions">
          <PrimaryLink href="/">На главную</PrimaryLink>
          <PrimaryLink href="/download" variant="secondary">
            Скачать
          </PrimaryLink>
          <a className="text-link" href="/features">
            Возможности <Icon name="arrow" />
          </a>
        </div>
      </div>
    </section>
  );
}

function RouteLoading() {
  return (
    <section className="page route-loading">
      <p role="status">Загрузка…</p>
    </section>
  );
}

function normalizePathname(value: string) {
  try {
    return decodeURIComponent(value).replace(/\/+$/, "") || "/";
  } catch {
    return "/not-found";
  }
}

export function Site({
  pathname: providedPathname,
}: { pathname?: string } = {}) {
  const pathname = useMemo(() => {
    if (providedPathname) return normalizePathname(providedPathname);
    if (typeof window === "undefined") return "/";
    return normalizePathname(window.location.pathname);
  }, [providedPathname]);
  const resolved = useMemo(() => resolvePage(pathname), [pathname]);
  const privateRoute =
    resolved.page === "account" ||
    resolved.page === "settings" ||
    resolved.page === "reset" ||
    resolved.page === "callback";
  const { session, authLoading } = useAccountSession(privateRoute);

  let content: ReactNode;
  if (resolved.page === "home") content = <HomePage />;
  else if (resolved.page === "features") content = <FeaturesPage />;
  else if (resolved.page === "download") content = <DownloadPage />;
  else if (resolved.page === "news") content = <NewsPage />;
  else if (resolved.page === "article")
    content = <ArticlePage slug={resolved.article!.slug} />;
  else if (resolved.page === "account")
    content = <AuthPage session={session} />;
  else if (resolved.page === "settings")
    content = <AccountPage session={session} authLoading={authLoading} />;
  else if (resolved.page === "reset")
    content = <PasswordRecoveryPage session={session} />;
  else if (resolved.page === "callback")
    content = <CallbackPage authLoading={authLoading} session={session} />;
  else content = <NotFoundPage />;

  return (
    <PageFrame
      session={session}
      pathname={pathname}
      page={resolved.page}
      accent={resolved.accent}
      meta={resolved.meta}
    >
      <Suspense fallback={<RouteLoading />}>{content}</Suspense>
    </PageFrame>
  );
}

export function PrivateShell({ pathname }: { pathname: string }) {
  const normalized = normalizePathname(pathname);
  const resolved = useMemo(() => resolvePage(normalized), [normalized]);
  return (
    <PageFrame
      session={null}
      pathname={normalized}
      page={resolved.page}
      accent={resolved.accent}
      meta={resolved.meta}
    >
      <RouteLoading />
    </PageFrame>
  );
}
