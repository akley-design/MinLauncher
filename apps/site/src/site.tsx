import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import type { Session } from "@supabase/supabase-js";
import { articles } from "./news";
import { getWindowsDownload } from "./downloads";
import { supabase } from "./supabase";

type Provider = "google" | "discord";

function Icon({ name }: { name: "arrow" | "menu" | "close" | "spark" | "cube" | "bolt" | "check" }) {
  const paths = {
    arrow: <><path d="M4 8h8M8 4l4 4-4 4" /></>,
    menu: <><path d="M3 5h10M3 8h10M3 11h10" /></>,
    close: <><path d="m4 4 8 8M12 4l-8 8" /></>,
    spark: <><path d="m8 1 1.6 5.4L15 8l-5.4 1.6L8 15l-1.6-5.4L1 8l5.4-1.6z" /></>,
    cube: <><path d="m8 1 6 3.5v7L8 15l-6-3.5v-7z"/><path d="M2.3 4.7 8 8l5.7-3.3M8 8v7"/></>,
    bolt: <><path d="M9 1 3 9h4l-1 6 7-9H9z" /></>,
    check: <><path d="m3 8 3.2 3L13 4" /></>,
  };
  return <svg className="icon" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">{paths[name]}</svg>;
}

function ProviderMark({ provider }: { provider: Provider }) {
  return provider === "google" ? (
    <svg viewBox="0 0 18 18" aria-hidden="true" className="provider-mark"><path fill="#4285f4" d="M17.6 9.2c0-.6-.1-1.2-.2-1.8H9v3.4h4.8a4.2 4.2 0 0 1-1.8 2.7v2.2h2.9c1.7-1.6 2.7-3.9 2.7-6.5Z"/><path fill="#34a853" d="M9 18c2.4 0 4.4-.8 5.9-2.3L12 13.5c-.8.5-1.8.9-3 .9-2.3 0-4.2-1.5-4.9-3.6H1.1v2.3A9 9 0 0 0 9 18Z"/><path fill="#fbbc05" d="M4.1 10.8a5.4 5.4 0 0 1 0-3.6V4.9H1.1a9 9 0 0 0 0 8.2l3-2.3Z"/><path fill="#ea4335" d="M9 3.6c1.3 0 2.4.4 3.3 1.3l2.5-2.5A8.5 8.5 0 0 0 9 0a9 9 0 0 0-7.9 4.9l3 2.3C4.8 5.1 6.7 3.6 9 3.6Z"/></svg>
  ) : (
    <svg viewBox="0 0 24 24" aria-hidden="true" className="provider-mark provider-mark--discord" fill="currentColor"><path d="M19.7 5.1a18.7 18.7 0 0 0-4.6-1.4l-.6 1.2a17.1 17.1 0 0 0-5 0L8.9 3.7a18.7 18.7 0 0 0-4.6 1.4C1.4 9.4.6 13.6 1 17.8a18.8 18.8 0 0 0 5.7 2.9l1.2-2a12 12 0 0 1-1.9-.9l.5-.4a13.3 13.3 0 0 0 11 0l.5.4a12 12 0 0 1-1.9.9l1.2 2a18.8 18.8 0 0 0 5.7-2.9c.5-4.9-.8-9.1-3.3-12.7ZM8.7 14.6c-1 0-1.8-.9-1.8-2s.8-2 1.8-2 1.8.9 1.8 2-.8 2-1.8 2Zm6.6 0c-1 0-1.8-.9-1.8-2s.8-2 1.8-2 1.8.9 1.8 2-.8 2-1.8 2Z"/></svg>
  );
}

function ProviderButtons({ onError, busy, setBusy }: { onError: (message: string) => void; busy: boolean; setBusy: (busy: boolean) => void }) {
  async function signIn(provider: Provider) {
    if (!supabase) {
      onError("Вход пока не настроен. Администратору нужно добавить ключи Supabase в .env.");
      return;
    }
    onError("");
    setBusy(true);
    try {
      const { error } = await supabase.auth.signInWithOAuth({
        provider,
        options: { redirectTo: `${window.location.origin}/auth/callback` },
      });
      if (error) onError(error.message);
    } catch (error) {
      onError(error instanceof Error ? error.message : "Не удалось начать вход. Попробуй ещё раз.");
    } finally {
      setBusy(false);
    }
  }

  return <div className="provider-list">
    <button className="provider-button" type="button" disabled={busy} onClick={() => void signIn("google")}><ProviderMark provider="google"/><span>Продолжить с Google</span><Icon name="arrow"/></button>
    <button className="provider-button provider-button--discord" type="button" disabled={busy} onClick={() => void signIn("discord")}><ProviderMark provider="discord"/><span>Продолжить с Discord</span><Icon name="arrow"/></button>
  </div>;
}

function ArticleCard({ article, index = 0 }: { article: (typeof articles)[number]; index?: number }) {
  return <a className={`article-card article-card--${article.accent}`} href={`/news/${article.slug}`}>
    <div className="article-card__visual"><span className="article-card__number">0{index + 1}</span><span className="article-card__orb"/><span className="article-card__icon"><Icon name={index === 0 ? "bolt" : index === 1 ? "cube" : "spark"}/></span><span className="article-card__read">{article.readTime} <Icon name="arrow"/></span></div>
    <div className="article-card__body"><span className="eyebrow">{article.category}</span><h3>{article.title}</h3><p>{article.excerpt}</p><span className="text-link">Читать <Icon name="arrow"/></span></div>
  </a>;
}

function Header({ session }: { session: Session | null }) {
  const [menuOpen, setMenuOpen] = useState(false);
  const current = window.location.pathname;
  const links = [{ href: "/features", label: "Возможности" }, { href: "/download", label: "Скачать" }, { href: "/news", label: "Журнал" }];
  return <header className="site-header">
    <a className="brand" href="/" aria-label="MinLauncher — главная"><img src="/icon.png" alt=""/><span>MIN<span>LAUNCHER</span></span><i/></a>
    <nav className={menuOpen ? "site-nav site-nav--open" : "site-nav"} aria-label="Основная навигация">
      {links.map((link) => <a key={link.href} className={current.startsWith(link.href) ? "site-nav__link is-active" : "site-nav__link"} href={link.href} onClick={() => setMenuOpen(false)}>{link.label}</a>)}
      {session ? <a className="nav-account" href="/account/settings"><span className="nav-avatar">{(session.user.user_metadata.display_name ?? session.user.email ?? "M").toString().slice(0, 1).toUpperCase()}</span>Мой профиль</a> : <a className="nav-account" href="/account"><span className="nav-account__dot"/>Войти</a>}
    </nav>
    <a className="header-download" href="/download">Скачать <Icon name="arrow"/></a>
    <button className="menu-toggle" type="button" aria-label={menuOpen ? "Закрыть меню" : "Открыть меню"} aria-expanded={menuOpen} onClick={() => setMenuOpen((open) => !open)}><Icon name={menuOpen ? "close" : "menu"}/></button>
  </header>;
}

function Footer() {
  return <footer className="site-footer"><a className="brand brand--footer" href="/"><img src="/icon.png" alt=""/><span>MIN<span>LAUNCHER</span></span><i/></a><span className="footer-caption">Версии, профили и настройки.</span><div className="footer-links"><a href="/features">Возможности</a><a href="/download">Скачать</a><a href="/news">Журнал</a><a href="/account">Аккаунт</a><a href="https://github.com/akley-design/MinLauncher" target="_blank" rel="noreferrer">GitHub ↗</a></div><span className="copyright">© MinLauncher</span></footer>;
}

function PageFrame({ children, session }: { children: ReactNode; session: Session | null }) {
  return <><a className="skip-link" href="#main-content">К содержимому</a><Header session={session}/><main id="main-content" tabIndex={-1}>{children}</main><Footer/></>;
}

function PrimaryLink({ href, children, variant = "primary" }: { href: string; children: ReactNode; variant?: "primary" | "secondary" }) {
  return <a className={`button button--${variant}`} href={href}>{children}<Icon name="arrow"/></a>;
}

function HomePage() {
  return <>
    <section className="hero">
      <div className="hero__image" aria-hidden="true"/><div className="hero__grain" aria-hidden="true"/>
      <div className="hero-preview" aria-hidden="true">
        <div className="hero-preview__window">
          <div className="hero-preview__bar"><img src="/icon.png" alt=""/><span>MINLAUNCHER</span><div><i/><i/><i/></div></div>
          <div className="hero-preview__body">
            <nav><b>Играть</b><span>Версии</span><span>Инстансы</span><span>Настройки</span></nav>
            <div className="hero-preview__main">
              <div className="hero-preview__status"><i/> ГОТОВ К ЗАПУСКУ</div>
              <span className="hero-preview__eyebrow">ИГРОВОЙ ЦЕНТР</span>
              <h2>Выбери<br/>версию игры.</h2>
              <div className="hero-preview__version"><span>ВЫБРАННАЯ ВЕРСИЯ</span><b>Последний релиз</b><small>Java настроена</small></div>
              <div className="hero-preview__play">Играть <Icon name="arrow"/></div>
              <div className="hero-preview__foot"><span>Профили</span><span>Производительность</span></div>
            </div>
          </div>
        </div>
        <span className="hero-preview__note">ОДНО ОКНО · ТРИ ШАГА ДО ИГРЫ</span>
      </div>
      <div className="hero__content"><div className="hero__status"><span className="live-dot"/> Лаунчер для Minecraft</div>
        <p className="hero__overline">MinLauncher <span> / </span> версии и игровые настройки</p>
        <h1>Версии<br/>и профили<span className="hero__period">.</span></h1>
        <p className="hero__summary">Выбирай версии Minecraft, настраивай игровые профили и следи за производительностью в одном месте.</p>
        <div className="hero__actions"><PrimaryLink href="/download">Скачать MinLauncher</PrimaryLink><PrimaryLink href="/features" variant="secondary">Узнать возможности</PrimaryLink></div>
        <div className="hero__meta"><span><b>01</b> Скачай</span><span><b>02</b> Выбери версию</span><span><b>03</b> Запусти</span></div>
      </div>
      <div className="hero__side-label">Версии, профили и настройки Minecraft</div>
      <a className="hero__scroll" href="#manifesto"><span/> Подробнее</a>
    </section>
    <section className="manifesto section-wrap" id="manifesto"><div className="manifesto__top"><span className="eyebrow"><i/> Как устроен MinLauncher</span><span className="manifesto__index">01 / 04</span></div><div className="manifesto__grid"><h2>ВЕРСИИ ИГРЫ.<br/><em>ОДНО ОКНО.</em></h2><div className="manifesto__copy"><p className="lead">Выбери версию, настрой игру и запусти её из одного окна.</p><p>MinLauncher хранит версии и игровые профили, помогает настроить производительность и показывает результаты замеров.</p><a className="text-link" href="/features">Возможности <Icon name="arrow"/></a></div></div><div className="manifesto__ticker"><div><span>Версии Minecraft</span><b>·</b><span>Игровые профили</span><b>·</b><span>Производительность</span></div></div></section>
    <section className="feature-showcase section-wrap"><div className="section-heading"><div><span className="eyebrow">02 / Возможности</span><h2>НАСТРОЙКИ.<br/><em>ЗАМЕРЫ.</em></h2></div><p>Выбери версию, создай игровой профиль и настрой параметры игры.</p></div><div className="feature-grid"><article className="feature-card feature-card--launch"><span className="feature-card__number">01 / СТАРТ</span><div className="feature-art feature-art--orbit"><span/><span/><span/><Icon name="bolt"/></div><h3>БЫСТРЫЙ<br/>ЗАПУСК</h3><p>Повторная загрузка использует уже скачанные библиотеки и ресурсы.</p><a href="/features" aria-label="Узнать о быстром запуске"><Icon name="arrow"/></a></article><article className="feature-card feature-card--versions"><span className="feature-card__number">02 / ПОРЯДОК</span><div className="feature-art feature-art--blocks"><i/><i/><i/><i/><i/></div><h3>ВЕРСИИ И<br/>ИНСТАНСЫ</h3><p>Версии Minecraft и игровые инстансы доступны в одном окне.</p><a href="/features" aria-label="Узнать об управлении версиями"><Icon name="arrow"/></a></article><article className="feature-card feature-card--perf"><span className="feature-card__number">03 / КОНТРОЛЬ</span><div className="feature-art feature-art--wave"><span/><span/><span/><span/><span/><span/><span/></div><h3>ЗАМЕРЫ<br/>И РЕЗУЛЬТАТЫ</h3><p>Результаты замеров сохраняются вместе с условиями.</p><a href="/features" aria-label="Узнать об оптимизации"><Icon name="arrow"/></a></article></div></section>
    <section className="manifesto-quote"><div className="quote__spark"><Icon name="spark"/></div><span className="eyebrow">Проверка запуска</span><blockquote>«ЗАПУСК ПРОВЕРЯЕТСЯ<br/>ПО ЗАРАНЕЕ ОПИСАННОЙ<br/><em>МЕТОДИКЕ.</em></blockquote><a href="/news/speed-without-magic">Методика измерений <Icon name="arrow"/></a></section>
    <section className="journal section-wrap"><div className="section-heading section-heading--journal"><div><span className="eyebrow">03 / Журнал MinLauncher</span><h2>Заметки о<br/><em>проекте.</em></h2></div><PrimaryLink href="/news" variant="secondary">Все материалы</PrimaryLink></div><div className="article-grid">{articles.slice(0, 2).map((article, index) => <ArticleCard key={article.slug} article={article} index={index}/>)}</div></section>
    <section className="final-cta"><div className="final-cta__flare"/><span className="eyebrow">ДЛЯ WINDOWS</span><h2>СКАЧАЙ<br/><em>MINLAUNCHER.</em></h2><PrimaryLink href="/download">Скачать лаунчер</PrimaryLink></section>
  </>;
}

function FeaturesPage() {
  const features = [
    { n: "01", title: "Повторное использование файлов", text: "Лаунчер повторно использует загруженные библиотеки и ресурсы.", icon: "bolt" as const, tag: "УМНЫЙ КЭШ" },
    { n: "02", title: "Отдельные игровые профили", text: "У каждого инстанса свои файлы, версия Minecraft и настройки.", icon: "cube" as const, tag: "ВЕРСИИ И ИНСТАНСЫ" },
    { n: "03", title: "Меняй настройки игры", text: "Профиль «Максимум кадров» снижает тяжёлые эффекты, отключает VSync и не ограничивает частоту кадров.", icon: "spark" as const, tag: "НАСТРОЙКИ ИГРЫ" },
    { n: "04", title: "Проверяй результат", text: "Метрики и сценарии замеров описаны заранее. Каждый результат включает сведения о машине и условиях.", icon: "check" as const, tag: "ИЗМЕРИМЫЙ РЕЗУЛЬТАТ" },
  ];
  return <PageIntro index="01" label="ВОЗМОЖНОСТИ" title={<>Что умеет<br/><em>MinLauncher.</em></>} intro="Выбирай версии Minecraft, настраивай профили и запускай игру."><div className="features-list">{features.map((item) => <article className="features-row" key={item.n}><span className="features-row__n">{item.n}</span><div className="features-row__icon"><Icon name={item.icon}/></div><div className="features-row__body"><span className="eyebrow">{item.tag}</span><h2>{item.title}</h2><p>{item.text}</p></div><span className="features-row__arrow"><Icon name="arrow"/></span></article>)}</div><div className="page-bottom-cta"><span>MINLAUNCHER ДЛЯ WINDOWS</span><PrimaryLink href="/download">Скачать лаунчер</PrimaryLink></div></PageIntro>;
}

function PageIntro({ index, label, title, intro, children }: { index: string; label: string; title: ReactNode; intro: string; children: ReactNode }) {
  return <section className="inner-page section-wrap"><div className="inner-page__top"><span className="eyebrow"><i/> {label}</span><span className="manifesto__index">{index} / 04</span></div><div className="inner-page__heading"><h1>{title}</h1><p>{intro}</p></div>{children}</section>;
}

function DownloadPage() {
  const [url, setUrl] = useState<string | null>(null);
  const [status, setStatus] = useState("Проверяем последний релиз");
  useEffect(() => {
    const controller = new AbortController();
    void getWindowsDownload(controller.signal).then((download) => {
      setUrl(download);
      setStatus("Последняя стабильная версия · Windows x64");
    }).catch(() => {
      if (!controller.signal.aborted) setStatus("Подписанный установщик пока не опубликован");
    });
    return () => controller.abort();
  }, []);
  return <PageIntro index="02" label="ЗАГРУЗКА" title={<>Скачать<br/><em>MinLauncher.</em></>} intro="Скачай лаунчер. При первом запуске он поможет установить выбранную версию Minecraft."><div className="download-panel"><div className="download-panel__orb"><img src="/icon.png" alt=""/></div><div className="download-panel__details"><span className="eyebrow">ДЛЯ КОМПЬЮТЕРА</span><h2>Windows <em>·</em> x64</h2><p>Подписанный установщик MinLauncher. Ссылка ведёт на последний опубликованный релиз.</p><span className="download-panel__status" role="status"><i/> {status}</span>{url ? <a className="button button--primary" href={url}>Скачать для Windows <Icon name="arrow"/></a> : <button className="button button--primary download-disabled" type="button" disabled>Установщик скоро появится</button>}</div><div className="download-panel__side">ML<br/><span>0.1.0</span></div></div><p className="download-note"><Icon name="check"/> Установщик появится здесь после публикации подписанного GitHub-релиза.</p></PageIntro>;
}

function NewsPage() {
  return <PageIntro index="03" label="ЖУРНАЛ" title={<>Заметки о<br/><em>MinLauncher.</em></>} intro="Пишем о версиях Minecraft, игровых настройках и замерах запуска."><div className="article-grid article-grid--all">{articles.map((article, index) => <ArticleCard key={article.slug} article={article} index={index}/>)}</div></PageIntro>;
}

function ArticlePage({ slug }: { slug: string }) {
  const article = articles.find((item) => item.slug === slug);
  if (!article) return <section className="not-found section-wrap"><span className="eyebrow">404 / ТАКОЙ СТРАНИЦЫ НЕТ</span><h1>СТРАНИЦА<br/><em>НЕ НАЙДЕНА.</em></h1><PrimaryLink href="/news" variant="secondary">Вернуться в журнал</PrimaryLink></section>;
  return <article className="article-page section-wrap"><a className="back-link" href="/news">← К журналу</a><span className="eyebrow">{article.category} <span className="article-page__read">· {article.readTime}</span></span><h1>{article.title}</h1><p className="article-page__excerpt">{article.excerpt}</p><div className={`article-page__art article-page__art--${article.accent}`} aria-hidden="true"><span className="article-art__step"><b>01</b><i/><small>Версия</small></span><span className="article-art__step"><b>02</b><i/><small>Профиль</small></span><span className="article-art__step"><b>03</b><i/><small>Запуск</small></span></div><div className="article-page__body">{article.body.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}<PrimaryLink href="/download" variant="secondary">Попробовать MinLauncher</PrimaryLink></div></article>;
}

function AuthPage({ session }: { session: Session | null }) {
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  if (session) return <section className="auth-page"><div className="auth-card"><span className="auth-card__stamp"><Icon name="check"/></span><span className="eyebrow">АККАУНТ АКТИВЕН</span><h1>АККАУНТ<br/><em>ПОДКЛЮЧЁН.</em></h1><p>Вы вошли как {session.user.email ?? "пользователь MinLauncher"}.</p><PrimaryLink href="/account/settings">Настройки профиля</PrimaryLink></div></section>;
  return <section className="auth-page"><div className="auth-art" aria-hidden="true"><span>Версия игры<br/>и её настройки</span><i/><div className="auth-art__label">ПОСЛЕ ВХОДА</div><div className="auth-art__steps"><div><b>01</b><span>Выбери версию</span><i>↗</i></div><div><b>02</b><span>Настрой профиль</span><i>↗</i></div><div><b>03</b><span>Запускай игру</span><i>↗</i></div></div></div><div className="auth-card"><a className="back-link" href="/">← На главную</a><span className="eyebrow">Аккаунт MinLauncher</span><h1>Войти<br/><em>в аккаунт.</em></h1><p>Войди или создай аккаунт. После входа он будет доступен и в лаунчере.</p><ProviderButtons onError={setError} busy={busy} setBusy={setBusy}/>{error && <p className="form-message form-message--error" role="alert">{error}</p>}<span className="auth-terms">Для входа откроется страница выбранного сервиса. MinLauncher не получает твой пароль.</span></div></section>;
}

function AccountPage({ session, authLoading }: { session: Session | null; authLoading: boolean }) {
  const [name, setName] = useState("");
  const [channel, setChannel] = useState("stable");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const metadata = session?.user.user_metadata ?? {};

  useEffect(() => {
    setName(String(metadata.display_name ?? metadata.full_name ?? ""));
    setChannel(String(metadata.release_channel ?? "stable"));
  }, [session?.user.id, metadata.display_name, metadata.full_name, metadata.release_channel]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!supabase || !session) return;
    setBusy(true);
    setMessage("");
    setError("");
    try {
      const { error: saveError } = await supabase.auth.updateUser({ data: { ...metadata, display_name: name.trim(), release_channel: channel } });
      if (saveError) setError(saveError.message);
      else setMessage("Настройки профиля сохранены.");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Не удалось сохранить настройки.");
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
      setError(signOutError instanceof Error ? signOutError.message : "Не удалось выйти из аккаунта.");
    } finally {
      setBusy(false);
    }
  }

  if (authLoading) return <section className="account-page section-wrap"><span className="eyebrow">ПРОВЕРЯЕМ АККАУНТ…</span></section>;
  if (!session) return <section className="account-page section-wrap"><div className="account-empty"><span className="eyebrow">ЛИЧНЫЙ ПРОФИЛЬ</span><h1>ПРОФИЛЬ<br/><em>MINLAUNCHER.</em></h1><p>Войди, чтобы изменить имя и канал обновлений.</p><PrimaryLink href="/account">Войти в аккаунт</PrimaryLink></div></section>;

  return <section className="account-page section-wrap"><div className="inner-page__top"><span className="eyebrow"><i/> ЛИЧНЫЙ ПРОФИЛЬ</span><span className="manifesto__index">ACCOUNT / SETTINGS</span></div><div className="account-heading"><div className="account-avatar">{(name || session.user.email || "M").slice(0, 1).toUpperCase()}</div><div><span className="eyebrow">MINLAUNCHER ACCOUNT</span><h1>НАСТРОЙКИ<br/><em>ПРОФИЛЯ.</em></h1><p>{session.user.email}</p></div></div><form className="settings-form" onSubmit={(event) => void save(event)}><label className="field"><span>ИМЯ В ПРОФИЛЕ</span><input maxLength={40} value={name} onChange={(event) => setName(event.target.value)} placeholder="Как к тебе обращаться?" autoComplete="nickname"/></label><label className="field"><span>КАНАЛ ОБНОВЛЕНИЙ</span><select value={channel} onChange={(event) => setChannel(event.target.value)}><option value="stable">Стабильный</option><option value="preview">Предварительный доступ</option></select></label><div className="field field--email"><span>АККАУНТ</span><p>{session.user.email}</p><small>Адрес почты управляется Google или Discord.</small></div><div className="settings-actions"><button className="button button--primary" type="submit" disabled={busy || !supabase}>{busy ? "СОХРАНЕНИЕ…" : "Сохранить настройки"}<Icon name="check"/></button><button className="button button--ghost" type="button" onClick={() => void signOut()} disabled={busy}>Выйти из аккаунта</button></div>{message && <p className="form-message form-message--success" role="status">{message}</p>}{error && <p className="form-message form-message--error" role="alert">{error}</p>}</form><p className="account-disclaimer">Канал обновлений сохраняется в профиле сайта. Он пока не меняет настройки установленного приложения.</p></section>;
}

function CallbackPage({ authLoading, session }: { authLoading: boolean; session: Session | null }) {
  const [error, setError] = useState("");
  const [handoffUrl, setHandoffUrl] = useState("");
  const [handoffBusy, setHandoffBusy] = useState(false);
  const [handoffError, setHandoffError] = useState("");
  const startedFor = useRef("");

  const createHandoff = useCallback(async () => {
    if (!supabase || !session) return;
    setHandoffBusy(true);
    setHandoffError("");
    try {
      const { data, error: functionError } = await supabase.functions.invoke("create-handoff");
      if (functionError) throw functionError;
      if (typeof data?.code !== "string") throw new Error("Сервер не выдал ссылку для входа.");
      const url = `minlauncher://auth/transfer?code=${encodeURIComponent(data.code)}`;
      setHandoffUrl(url);
      window.location.href = url;
    } catch (cause) {
      setHandoffError(cause instanceof Error ? cause.message : "Не удалось подготовить вход в лаунчер.");
    } finally {
      setHandoffBusy(false);
    }
  }, [session]);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const hash = new URLSearchParams(window.location.hash.replace(/^#/, ""));
    setError(params.get("error_description") ?? hash.get("error_description") ?? "");
    if (!authLoading && session && startedFor.current !== session.user.id) {
      startedFor.current = session.user.id;
      void createHandoff();
    }
  }, [authLoading, session?.user.id, createHandoff]);

  return <section className="auth-page"><div className="auth-card"><span className="eyebrow">MINLAUNCHER ACCOUNT</span><h1>{error ? <>ВХОД<br/><em>НЕ УДАЛСЯ.</em></> : session ? <>АККАУНТ<br/><em>ГОТОВ.</em></> : <>СЕССИЯ<br/><em>НЕ НАЙДЕНА.</em></>}</h1><p>{error || (authLoading ? "Проверяем защищённый вход…" : session ? "Открываем MinLauncher. Ссылка действует одну минуту и используется один раз." : "Сессия не найдена. Попробуй войти ещё раз.")}</p>{handoffUrl && <a className="button button--primary" href={handoffUrl}>Открыть MinLauncher <Icon name="arrow"/></a>}{!authLoading && !session && <PrimaryLink href="/account">Вернуться ко входу</PrimaryLink>}{session && <button className="button button--ghost" type="button" disabled={handoffBusy} onClick={() => void createHandoff()}>{handoffBusy ? "ГОТОВИМ ССЫЛКУ…" : "ОБНОВИТЬ ССЫЛКУ И ОТКРЫТЬ ЛАУНЧЕР"}<Icon name="arrow"/></button>}{handoffError && <p className="form-message form-message--error" role="alert">{handoffError}</p>}</div></section>;
}

function NotFoundPage() {
  return <section className="not-found section-wrap"><span className="eyebrow">404 / ТАКОЙ СТРАНИЦЫ НЕТ</span><h1>СТРАНИЦА<br/><em>НЕ НАЙДЕНА.</em></h1><PrimaryLink href="/" variant="secondary">На главную</PrimaryLink></section>;
}

export function Site() {
  const [session, setSession] = useState<Session | null>(null);
  const [authLoading, setAuthLoading] = useState(Boolean(supabase));
  const pathname = useMemo(() => {
    try { return decodeURIComponent(window.location.pathname).replace(/\/+$/, "") || "/"; }
    catch { return "/not-found"; }
  }, []);

  useEffect(() => {
    if (!supabase) return;
    let active = true;
    const { data: { subscription } } = supabase.auth.onAuthStateChange((_event, nextSession) => {
      if (active) { setSession(nextSession); setAuthLoading(false); }
    });
    void supabase.auth.getSession().then(({ data, error }) => {
      if (active) {
        setSession(data.session);
        setAuthLoading(false);
        if (error) console.error("Could not restore Supabase session", error);
      }
    }).catch((error: unknown) => {
      if (active) {
        console.error("Could not restore Supabase session", error);
        setAuthLoading(false);
      }
    });
    return () => { active = false; subscription.unsubscribe(); };
  }, []);

  const [page, slug] = pathname.split("/").filter(Boolean);
  let content: ReactNode;
  if (pathname === "/") content = <HomePage/>;
  else if (pathname === "/features") content = <FeaturesPage/>;
  else if (pathname === "/download") content = <DownloadPage/>;
  else if (pathname === "/news") content = <NewsPage/>;
  else if (page === "news" && slug) content = <ArticlePage slug={slug}/>;
  else if (pathname === "/account") content = <AuthPage session={session}/>;
  else if (pathname === "/account/settings") content = <AccountPage session={session} authLoading={authLoading}/>;
  else if (pathname === "/auth/callback") content = <CallbackPage authLoading={authLoading} session={session}/>;
  else content = <NotFoundPage/>;

  return <PageFrame session={session}>{content}</PageFrame>;
}
