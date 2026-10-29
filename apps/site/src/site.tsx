import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import type { Session } from "@supabase/supabase-js";
import { AuthPage, PasswordRecoveryPage } from "./account-auth";
import { nicknamePattern, nicknamePatternSource } from "./auth-form";
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
import {
  SiteDiorama,
  sceneCoversSite,
  sceneWidesSite,
} from "@minlauncher/visuals/site";
import { supabase } from "./supabase";

function Icon({
  name,
}: {
  name: "arrow" | "menu" | "close" | "spark" | "cube" | "bolt" | "check";
}) {
  const paths = {
    arrow: (
      <>
        <path d="M4 8h8M8 4l4 4-4 4" />
      </>
    ),
    menu: (
      <>
        <path d="M3 5h10M3 8h10M3 11h10" />
      </>
    ),
    close: (
      <>
        <path d="m4 4 8 8M12 4l-8 8" />
      </>
    ),
    spark: (
      <>
        <path d="m8 1 1.6 5.4L15 8l-5.4 1.6L8 15l-1.6-5.4L1 8l5.4-1.6z" />
      </>
    ),
    cube: (
      <>
        <path d="m8 1 6 3.5v7L8 15l-6-3.5v-7z" />
        <path d="M2.3 4.7 8 8l5.7-3.3M8 8v7" />
      </>
    ),
    bolt: (
      <>
        <path d="M9 1 3 9h4l-1 6 7-9H9z" />
      </>
    ),
    check: (
      <>
        <path d="m3 8 3.2 3L13 4" />
      </>
    ),
  };
  return (
    <svg
      className="icon"
      viewBox="0 0 16 16"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {paths[name]}
    </svg>
  );
}

function ArticleCard({
  article,
  index = 0,
  variant = "default",
}: {
  article: NewsArticle;
  index?: number;
  variant?: "default" | "feature" | "row";
}) {
  const cover = sceneCoversSite[article.scene];
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
          <img src={cover} alt="" loading="lazy" />
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
        <img src={cover} alt="" />
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

function PrimaryLink({
  href,
  children,
  variant = "primary",
}: {
  href: string;
  children: ReactNode;
  variant?: "primary" | "secondary";
}) {
  return (
    <a className={`button button--${variant}`} href={href}>
      {children}
      <Icon name="arrow" />
    </a>
  );
}

function HomePage() {
  return (
    <>
      <section className="hero section-wrap">
        <div className="hero__content">
          <span className="eyebrow page-enter">
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
          <SiteDiorama variant="island" />
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
          <img
            src="/launcher-preview.png"
            alt="Интерфейс MinLauncher: выбор версии Minecraft и запуск игры"
            loading="lazy"
          />
          <figcaption>Игра, версии и настройки — в одном окне.</figcaption>
        </figure>
      </section>
      <section className="feature-story section-wrap">
        <div className="story-art">
          <SiteDiorama variant="cave" />
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
          <div className="page-kicker page-enter">
            <span className="eyebrow">
              <i /> Полевой справочник
            </span>
            <span className="page-index">01 / Возможности</span>
          </div>
          <h1 className="page-display page-enter">
            Что внутри
            <br />
            <em>лаунчера.</em>
          </h1>
          <p className="page-lede page-enter">
            Версии, изолированные профили, Java, память и замеры запуска — без
            лишних папок и ручной сборки аргументов.
          </p>
          <dl className="feature-stats page-enter">
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
        <div className="feature-hero__art page-art page-enter">
          <img src={sceneWidesSite.cave} alt="" />
          <span className="art-caption">CAVE / FIELD GUIDE</span>
        </div>
      </header>
      <div className="feature-board section-wrap">
        <div className="feature-stage">
          <SiteDiorama variant="cave" />
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
    const timer = window.setInterval(update, 20);
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
        <div className="page-kicker page-enter">
          <span className="eyebrow">
            <i /> MinLauncher / Загрузка
          </span>
          <span className="page-index">03 / Windows x64</span>
        </div>
        <h1 className="product-heading page-display page-enter">
          Скачать
          <br />
          <em>MinLauncher.</em>
        </h1>
        <p className="download-intro page-lede page-enter">
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
          <img
            className="download-stage__wide"
            src={sceneWidesSite.portal}
            alt=""
          />
          <div className="download-stage__scene">
            <SiteDiorama variant="portal" />
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
        <div className="page-kicker page-enter">
          <span className="eyebrow">
            <i /> Журнал
          </span>
          <span className="page-index">02 / Заметки</span>
        </div>
        <div className="news-masthead__grid">
          <h1 className="page-display page-enter">
            Заметки
            <br />
            <em>и наблюдения.</em>
          </h1>
          <div className="news-issue page-enter">
            <span>Выпуск</span>
            <strong>{String(articles.length).padStart(2, "0")}</strong>
            <span>записи · MinLauncher</span>
          </div>
        </div>
        <p className="page-lede page-enter">
          О релизах, производительности и деталях, которые обычно остаются за
          кадром.
        </p>
      </header>
      <div className="news-feature section-wrap">
        <ArticleCard article={featured} variant="feature" />
      </div>
      <section className="news-world section-wrap" aria-label="Наблюдение">
        <div className="news-world__art" aria-hidden="true">
          <img src={sceneWidesSite.cave} alt="" loading="lazy" />
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

function ArticlePage({ slug }: { slug: string }) {
  const article = articles.find((item) => item.slug === slug);
  if (!article) return <NotFoundPage />;
  const wide = sceneWidesSite[article.scene];
  const related = articles.filter((item) => item.slug !== slug);
  return (
    <article className={`page article-page article-page--${article.scene}`}>
      <header className="article-hero section-wrap">
        <a className="back-link" href="/news">
          ← Журнал
        </a>
        <div className="page-kicker page-enter">
          <span className="eyebrow">
            <i /> {article.category}
          </span>
          <span className="page-index">{article.readTime} / Журнал</span>
        </div>
        <h1 className="article-hero__title page-enter">{article.title}</h1>
        <p className="article-hero__excerpt page-enter">{article.excerpt}</p>
      </header>
      <div className="article-hero__art section-wrap">
        <img src={wide} alt="" />
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
              <dt>Журнал</dt>
              <dd>MinLauncher</dd>
            </div>
          </dl>
        </aside>
        <div className="article-body">
          {article.body.map((paragraph) => (
            <p key={paragraph}>{paragraph}</p>
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

function NicknameForm({
  initial = "",
  onSaved,
  submitLabel = "Сохранить ник",
}: {
  initial?: string;
  onSaved: (profile: { nickname: string; player_uuid: string }) => void;
  submitLabel?: string;
}) {
  const [nickname, setNickname] = useState(initial);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [checking, setChecking] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const valid = nicknamePattern.test(nickname);

  useEffect(() => {
    setNickname(initial);
  }, [initial]);

  useEffect(() => {
    setAvailable(null);
    const auth = supabase;
    if (!auth || !valid) return;
    let active = true;
    const timer = window.setTimeout(() => {
      setChecking(true);
      void auth.functions
        .invoke("launcher-nickname", {
          body: { operation: "check", nickname },
        })
        .then(({ data, error: checkError }) => {
          if (!active) return;
          if (checkError) setError(checkError.message);
          else setAvailable(data?.available === true);
        })
        .finally(() => {
          if (active) setChecking(false);
        });
    }, 350);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [nickname, valid]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !valid || available !== true) return;
    setBusy(true);
    setError("");
    const { data, error: saveError } = await supabase.functions.invoke(
      "launcher-nickname",
      {
        body: { operation: "claim_profile", nickname },
      },
    );
    setBusy(false);
    if (saveError) setError(saveError.message);
    else if (data?.profile?.nickname && data?.profile?.player_uuid)
      onSaved(data.profile);
    else setError("Сервер не подтвердил ник.");
  }

  return (
    <form className="settings-form" onSubmit={(event) => void save(event)}>
      <label className="field">
        <span>НИК MINLAUNCHER</span>
        <input
          maxLength={16}
          value={nickname}
          onChange={(event) => setNickname(event.target.value)}
          placeholder="Например, Steve_1"
          autoComplete="nickname"
        />
      </label>
      <small role="status">
        {!valid
          ? "3–16 латинских букв, цифр или _"
          : checking
            ? "Проверяем…"
            : available === true
              ? "Ник свободен"
              : available === false
                ? "Ник уже занят"
                : ""}
      </small>
      <small>
        Ник проверяется в реестре MinLauncher и не связан с проверкой аккаунтов
        Minecraft.
      </small>
      <button
        className="button button--primary"
        type="submit"
        disabled={busy || checking || available !== true}
      >
        {busy ? "СОХРАНЕНИЕ…" : submitLabel}
        <Icon name="check" />
      </button>
      {error && (
        <p className="form-message form-message--error" role="alert">
          {error}
        </p>
      )}
    </form>
  );
}

function AccountPage({
  session,
  authLoading,
}: {
  session: Session | null;
  authLoading: boolean;
}) {
  const [name, setName] = useState("");
  const [channel, setChannel] = useState("stable");
  const [registeredNickname, setRegisteredNickname] = useState("");
  const [nicknameAvailable, setNicknameAvailable] = useState<boolean | null>(
    null,
  );
  const [checkingNickname, setCheckingNickname] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [minecraftLinked, setMinecraftLinked] = useState<{
    uuid: string;
    username: string;
    linked_at: string;
  } | null>(null);
  const [minecraftBusy, setMinecraftBusy] = useState(false);
  const [minecraftHandoffUrl, setMinecraftHandoffUrl] = useState("");
  const [minecraftError, setMinecraftError] = useState("");
  const metadata = session?.user.user_metadata ?? {};

  useEffect(() => {
    setName(String(metadata.display_name ?? metadata.full_name ?? ""));
    setChannel(String(metadata.release_channel ?? "stable"));
    if (supabase && session) {
      void supabase.functions
        .invoke("launcher-nickname", { body: { operation: "get_profile" } })
        .then(({ data }) => {
          if (data?.profile?.nickname) {
            setRegisteredNickname(data.profile.nickname);
            setName(data.profile.nickname);
          }
        });
    }
  }, [
    session?.user.id,
    metadata.display_name,
    metadata.full_name,
    metadata.release_channel,
  ]);

  useEffect(() => {
    if (!supabase || !session) return;
    let active = true;
    void supabase.functions
      .invoke("minecraft-account-status")
      .then(({ data, error: statusError }) => {
        if (active && !statusError) setMinecraftLinked(data?.linked ?? null);
      });
    const params = new URLSearchParams(window.location.search);
    const handoffCode = params.get("minecraft_handoff");
    const oauthError = params.get("minecraft_error");
    if (!handoffCode && !oauthError)
      return () => {
        active = false;
      };
    window.history.replaceState(
      {},
      "",
      `${window.location.pathname}${window.location.hash}`,
    );
    if (oauthError) {
      const errors: Record<string, string> = {
        cancelled: "Вход Microsoft отменён.",
        already_linked_elsewhere:
          "Этот Microsoft-аккаунт уже привязан к другому профилю MinLauncher.",
        another_account_linked:
          "К профилю уже привязан другой Microsoft-аккаунт. Сначала отвяжите его.",
        minecraft_unavailable:
          "У аккаунта нет Minecraft Java Edition или Xbox-профиля, либо Microsoft не разрешил вход.",
        not_configured:
          "Вход Microsoft ещё не настроен на сервере MinLauncher.",
        expired: "Срок входа Microsoft истёк. Начните привязку заново.",
      };
      setMinecraftError(
        errors[oauthError] ?? "Microsoft не завершил вход. Попробуйте снова.",
      );
      return () => {
        active = false;
      };
    }
    if (!handoffCode)
      return () => {
        active = false;
      };
    setMinecraftBusy(true);
    setMinecraftError("");
    const handoff = `minlauncher://auth/minecraft?code=${encodeURIComponent(handoffCode)}`;
    setMinecraftHandoffUrl(handoff);
    window.location.href = handoff;
    setMinecraftBusy(false);
    return () => {
      active = false;
    };
  }, [session?.user.id]);

  async function connectMinecraft() {
    if (!supabase) return;
    setMinecraftBusy(true);
    setMinecraftError("");
    setMinecraftHandoffUrl("");
    try {
      const { data, error: startError } = await supabase.functions.invoke(
        "minecraft-oauth-start",
      );
      if (startError) throw startError;
      if (typeof data?.authorize_url !== "string")
        throw new Error("Microsoft-вход не настроен на сервере.");
      window.location.assign(data.authorize_url);
    } catch (cause) {
      setMinecraftError(
        cause instanceof Error
          ? cause.message
          : "Не удалось начать вход Microsoft.",
      );
      setMinecraftBusy(false);
    }
  }

  async function unlinkMinecraft() {
    if (!supabase) return;
    setMinecraftBusy(true);
    setMinecraftError("");
    try {
      const { error: unlinkError } = await supabase.functions.invoke(
        "minecraft-account-unlink",
      );
      if (unlinkError) throw unlinkError;
      setMinecraftLinked(null);
      setMinecraftHandoffUrl("");
    } catch (cause) {
      setMinecraftError(
        cause instanceof Error
          ? cause.message
          : "Не удалось отвязать Minecraft.",
      );
    } finally {
      setMinecraftBusy(false);
    }
  }

  useEffect(() => {
    const valid = nicknamePattern.test(name);
    setNicknameAvailable(null);
    if (!valid) return;
    if (
      registeredNickname &&
      name.toLowerCase() === registeredNickname.toLowerCase()
    ) {
      setNicknameAvailable(true);
      return;
    }
    const auth = supabase;
    if (!auth) return;
    let active = true;
    const timer = window.setTimeout(() => {
      setCheckingNickname(true);
      void auth.functions
        .invoke("launcher-nickname", {
          body: { operation: "check", nickname: name },
        })
        .then(({ data, error: checkError }) => {
          if (active && !checkError)
            setNicknameAvailable(data?.available === true);
        })
        .finally(() => {
          if (active) setCheckingNickname(false);
        });
    }, 350);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [name, registeredNickname]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !session) return;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const { data: result, error: profileError } =
        await supabase.functions.invoke("launcher-nickname", {
          body: { operation: "claim_profile", nickname: name.trim() },
        });
      if (profileError) throw profileError;
      if (!result?.profile?.nickname) throw new Error("Ник не сохранён.");
      const { error: saveError } = await supabase.auth.updateUser({
        data: {
          ...metadata,
          display_name: result.profile.nickname,
          release_channel: channel,
        },
      });
      if (saveError) setError(saveError.message);
      else setMessage("Настройки профиля сохранены.");
    } catch (saveError) {
      setError(
        saveError instanceof Error
          ? saveError.message
          : "Не удалось сохранить настройки.",
      );
    } finally {
      setBusy(false);
    }
  }

  async function signOut() {
    if (!supabase) return;
    setBusy(true);
    try {
      const { error: signOutError } = await supabase.auth.signOut();
      if (signOutError) setError(signOutError.message);
      else window.location.assign("/");
    } catch (signOutError) {
      setError(
        signOutError instanceof Error
          ? signOutError.message
          : "Не удалось выйти из аккаунта.",
      );
    } finally {
      setBusy(false);
    }
  }

  if (authLoading)
    return (
      <section className="page settings-page">
        <div className="settings-banner section-wrap">
          <div className="settings-banner__copy">
            <div className="page-kicker">
              <span className="eyebrow">ПРОВЕРЯЕМ АККАУНТ…</span>
              <span className="page-index">ACCOUNT / SETTINGS</span>
            </div>
          </div>
        </div>
      </section>
    );
  if (!session)
    return (
      <section className="page settings-page settings-page--empty">
        <div className="settings-banner section-wrap">
          <div className="settings-banner__art" aria-hidden="true">
            <img src={sceneWidesSite.island} alt="" />
          </div>
          <div className="settings-banner__copy page-enter">
            <div className="page-kicker">
              <span className="eyebrow">
                <i /> Личный профиль
              </span>
              <span className="page-index">ACCOUNT / SETTINGS</span>
            </div>
            <h1 className="page-display">
              Профиль
              <br />
              <em>MinLauncher.</em>
            </h1>
            <p className="page-lede">
              Войди, чтобы изменить имя и канал обновлений.
            </p>
            <PrimaryLink href="/account">Войти в аккаунт</PrimaryLink>
          </div>
        </div>
      </section>
    );

  return (
    <section className="page settings-page">
      <div className="settings-banner section-wrap">
        <div className="settings-banner__art" aria-hidden="true">
          <img src={sceneWidesSite.island} alt="" />
        </div>
        <div className="settings-banner__copy page-enter">
          <div className="page-kicker">
            <span className="eyebrow">
              <i /> Личный профиль
            </span>
            <span className="page-index">ACCOUNT / SETTINGS</span>
          </div>
          <h1 className="page-display">
            Настройки
            <br />
            <em>профиля.</em>
          </h1>
          <p className="settings-banner__email">{session.user.email}</p>
        </div>
      </div>
      <div className="settings-layout section-wrap">
        <aside className="settings-side">
          <div className="account-avatar">
            {(name || session.user.email || "M").slice(0, 1).toUpperCase()}
          </div>
          <div>
            <span className="eyebrow">MINLAUNCHER ACCOUNT</span>
            <p className="settings-side__name">{name || session.user.email}</p>
          </div>
          <dl className="settings-side__meta">
            <div>
              <dt>Почта</dt>
              <dd>{session.user.email}</dd>
            </div>
            <div>
              <dt>Канал</dt>
              <dd>{channel}</dd>
            </div>
            <div>
              <dt>Minecraft</dt>
              <dd>
                {minecraftLinked ? minecraftLinked.username : "не привязан"}
              </dd>
            </div>
          </dl>
        </aside>
        <form className="settings-form" onSubmit={(event) => void save(event)}>
          <label className="field">
            <span>НИК MINLAUNCHER</span>
            <input
              maxLength={16}
              pattern={nicknamePatternSource}
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Например, Steve_1"
              autoComplete="nickname"
            />
            <small role="status">
              {!nicknamePattern.test(name)
                ? "3–16 латинских букв, цифр или _"
                : checkingNickname
                  ? "Проверяем…"
                  : nicknameAvailable === true
                    ? "Ник свободен"
                    : nicknameAvailable === false
                      ? "Ник уже занят"
                      : ""}
            </small>
            <small>
              Занятость проверяется среди профилей MinLauncher, не в Minecraft.
            </small>
          </label>
          <label className="field">
            <span>КАНАЛ ОБНОВЛЕНИЙ</span>
            <select
              value={channel}
              onChange={(event) => setChannel(event.target.value)}
            >
              <option value="stable">Стабильный</option>
              <option value="preview">Предварительный доступ</option>
            </select>
          </label>
          <div className="field field--email">
            <span>АККАУНТ</span>
            <p>{session.user.email}</p>
            <small>
              Почта используется для подтверждения аккаунта и восстановления
              доступа.
            </small>
          </div>
          <div className="field field--email">
            <span>MINECRAFT JAVA</span>
            {minecraftLinked ? (
              <>
                <p>Подключён профиль {minecraftLinked.username}</p>
                <small>UUID: {minecraftLinked.uuid}</small>
                <button
                  className="button button--ghost"
                  type="button"
                  disabled={minecraftBusy}
                  onClick={() => void connectMinecraft()}
                >
                  {minecraftBusy
                    ? "ПОДГОТАВЛИВАЕМ ВХОД…"
                    : "Обновить вход Microsoft"}
                </button>
                <button
                  className="button button--ghost"
                  type="button"
                  disabled={minecraftBusy}
                  onClick={() => void unlinkMinecraft()}
                >
                  {minecraftBusy ? "ОБНОВЛЯЕМ…" : "Отвязать Minecraft"}
                </button>
              </>
            ) : (
              <>
                <p>Подключите Microsoft-аккаунт с Minecraft Java Edition.</p>
                <button
                  className="button button--ghost"
                  type="button"
                  disabled={minecraftBusy}
                  onClick={() => void connectMinecraft()}
                >
                  {minecraftBusy
                    ? "ПОДГОТАВЛИВАЕМ ВХОД…"
                    : "Привязать Microsoft / Minecraft"}
                </button>
              </>
            )}
            {minecraftHandoffUrl ? (
              <a className="button button--primary" href={minecraftHandoffUrl}>
                Открыть MinLauncher
              </a>
            ) : null}
            {minecraftError ? (
              <small className="form-message form-message--error" role="alert">
                {minecraftError}
              </small>
            ) : null}
            <small>
              Откроется официальный вход Microsoft. MinLauncher не просит пароль
              Microsoft; игровая сессия передаётся лаунчеру по одноразовой
              ссылке.
            </small>
          </div>
          <div className="settings-actions">
            <button
              className="button button--primary"
              type="submit"
              disabled={busy || !supabase || nicknameAvailable !== true}
            >
              {busy ? "СОХРАНЕНИЕ…" : "Сохранить настройки"}
              <Icon name="check" />
            </button>
            <button
              className="button button--ghost"
              type="button"
              onClick={() => void signOut()}
              disabled={busy}
            >
              Выйти из аккаунта
            </button>
          </div>
          {message && (
            <p className="form-message form-message--success" role="status">
              {message}
            </p>
          )}
          {error && (
            <p className="form-message form-message--error" role="alert">
              {error}
            </p>
          )}
        </form>
      </div>
      <p className="account-disclaimer section-wrap">
        Канал обновлений сохраняется в профиле сайта. Он пока не меняет
        настройки установленного приложения.
      </p>
    </section>
  );
}

function CallbackPage({
  authLoading,
  session,
}: {
  authLoading: boolean;
  session: Session | null;
}) {
  const [error, setError] = useState("");
  const [handoffUrl, setHandoffUrl] = useState("");
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [handoffError, setHandoffError] = useState("");
  const [needsNickname, setNeedsNickname] = useState(false);
  const [profileLoaded, setProfileLoaded] = useState(false);
  const startedFor = useRef("");

  const createHandoff = useCallback(async () => {
    if (!supabase || !session) return;
    setHandoffBusy(true);
    setHandoffError("");
    try {
      const { data, error: functionError } =
        await supabase.functions.invoke("create-handoff");
      if (functionError) throw functionError;
      if (typeof data?.code !== "string")
        throw new Error("Сервер не выдал ссылку для входа.");
      const url = `minlauncher://auth/transfer?code=${encodeURIComponent(data.code)}`;
      setHandoffUrl(url);
      window.location.href = url;
    } catch (cause) {
      setHandoffError(
        cause instanceof Error
          ? cause.message
          : "Не удалось подготовить вход в лаунчер.",
      );
    } finally {
      setHandoffBusy(false);
    }
  }, [session]);

  useEffect(() => {
    if (
      window.sessionStorage.getItem("minlauncher:password-recovery") === "1"
    ) {
      setProfileLoaded(true);
      return;
    }
    const params = new URLSearchParams(window.location.search);
    const hash = new URLSearchParams(window.location.hash.replace(/^#/, ""));
    setError(
      params.get("error_description") ?? hash.get("error_description") ?? "",
    );
    if (!authLoading && session && startedFor.current !== session.user.id) {
      startedFor.current = session.user.id;
      void (async () => {
        const { data, error: profileError } = await supabase!.functions.invoke(
          "launcher-nickname",
          {
            body: { operation: "get_profile" },
          },
        );
        if (profileError) setHandoffError(profileError.message);
        else if (data?.profile) void createHandoff();
        else {
          const requestedNickname =
            session.user.user_metadata.requested_nickname;
          if (typeof requestedNickname === "string") {
            const { data: claim, error: claimError } =
              await supabase!.functions.invoke("launcher-nickname", {
                body: {
                  operation: "claim_profile",
                  nickname: requestedNickname,
                },
              });
            if (!claimError && claim?.profile) void createHandoff();
            else {
              setHandoffError(
                "Этот ник уже занят. Выберите другой ник для профиля.",
              );
              setNeedsNickname(true);
            }
          } else setNeedsNickname(true);
        }
        setProfileLoaded(true);
      })();
    }
  }, [authLoading, session?.user.id, createHandoff]);

  return (
    <section className="auth-page auth-page--airlock">
      <div className="auth-wash" aria-hidden="true">
        <img src={sceneWidesSite.portal} alt="" />
      </div>
      <div className="auth-card">
        <span className="eyebrow">MINLAUNCHER ACCOUNT</span>
        <h1>
          {error ? (
            <>
              ВХОД
              <br />
              <em>НЕ УДАЛСЯ.</em>
            </>
          ) : session ? (
            <>
              АККАУНТ
              <br />
              <em>ГОТОВ.</em>
            </>
          ) : (
            <>
              СЕССИЯ
              <br />
              <em>НЕ НАЙДЕНА.</em>
            </>
          )}
        </h1>
        <p>
          {error ||
            (authLoading
              ? "Проверяем защищённый вход…"
              : session && needsNickname
                ? "Выбери ник MinLauncher. После сохранения профиль передастся в лаунчер."
                : session && !profileLoaded
                  ? "Проверяем профиль…"
                  : session
                    ? "Открываем MinLauncher. Ссылка действует одну минуту и используется один раз."
                    : "Сессия не найдена. Попробуй войти ещё раз.")}
        </p>
        {session && needsNickname ? (
          <NicknameForm
            onSaved={() => {
              setHandoffError("");
              setNeedsNickname(false);
              void createHandoff();
            }}
          />
        ) : null}
        {handoffUrl && (
          <a className="button button--primary" href={handoffUrl}>
            Открыть MinLauncher <Icon name="arrow" />
          </a>
        )}
        {!authLoading && !session && (
          <PrimaryLink href="/account">Вернуться ко входу</PrimaryLink>
        )}
        {session && (
          <button
            className="button button--ghost"
            type="button"
            disabled={handoffBusy}
            onClick={() => void createHandoff()}
          >
            {handoffBusy
              ? "ГОТОВИМ ССЫЛКУ…"
              : "ОБНОВИТЬ ССЫЛКУ И ОТКРЫТЬ ЛАУНЧЕР"}
            <Icon name="arrow" />
          </button>
        )}
        {handoffError && (
          <p className="form-message form-message--error" role="alert">
            {handoffError}
          </p>
        )}
      </div>
    </section>
  );
}

function NotFoundPage() {
  return (
    <section className="page not-found">
      <div className="not-found__art section-wrap" aria-hidden="true">
        <img src={sceneWidesSite.cave} alt="" />
        <span className="not-found__number">404</span>
      </div>
      <div className="not-found__copy section-wrap">
        <div className="page-kicker page-enter">
          <span className="eyebrow">
            <i /> Ошибка маршрута
          </span>
          <span className="page-index">404 / MinLauncher</span>
        </div>
        <h1 className="page-display page-enter">
          Страница
          <br />
          <em>потерялась.</em>
        </h1>
        <p className="page-lede page-enter">
          Такого адреса нет. Вернись на главную или забери лаунчер — там всё на
          месте.
        </p>
        <div className="not-found__actions page-enter">
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

export function Site() {
  const [session, setSession] = useState<Session | null>(null);
  const [authLoading, setAuthLoading] = useState(Boolean(supabase));
  const pathname = useMemo(() => {
    try {
      return (
        decodeURIComponent(window.location.pathname).replace(/\/+$/, "") || "/"
      );
    } catch {
      return "/not-found";
    }
  }, []);

  useEffect(() => {
    if (!supabase) return;
    let active = true;
    const {
      data: { subscription },
    } = supabase.auth.onAuthStateChange((event, nextSession) => {
      if (event === "PASSWORD_RECOVERY") {
        window.sessionStorage.setItem("minlauncher:password-recovery", "1");
        window.setTimeout(
          () => window.location.replace("/auth/reset-password"),
          0,
        );
      }
      if (active) {
        setSession(nextSession);
        setAuthLoading(false);
      }
    });
    void supabase.auth
      .getSession()
      .then(({ data, error }) => {
        if (active) {
          setSession(data.session);
          setAuthLoading(false);
          if (error) console.error("Could not restore Supabase session", error);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          console.error("Could not restore Supabase session", error);
          setAuthLoading(false);
        }
      });
    return () => {
      active = false;
      subscription.unsubscribe();
    };
  }, []);

  const resolved = useMemo(() => resolvePage(pathname), [pathname]);
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
      {content}
    </PageFrame>
  );
}
