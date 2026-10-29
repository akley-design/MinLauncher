type SceneVariant = "island" | "cave" | "portal";

export interface MediaImage {
  avif: string;
  avifSet: string;
  webp: string;
  webpSet: string;
  width: number;
  height: number;
  sizes: string;
}

export interface PreloadImage {
  href: string;
  type: string;
  srcSet: string;
  sizes: string;
}

const posterSizes = "(max-width: 720px) 92vw, 620px";
const wideSizes = "(max-width: 720px) 92vw, 960px";
const coverSizes = "(max-width: 720px) 92vw, 520px";
const previewSizes = "(max-width: 720px) 92vw, 960px";

function poster(variant: SceneVariant): MediaImage {
  return {
    avif: `/media/${variant}-960.avif`,
    avifSet: `/media/${variant}-480.avif 480w, /media/${variant}-720.avif 720w, /media/${variant}-960.avif 960w`,
    webp: `/media/${variant}-960.webp`,
    webpSet: `/media/${variant}-480.webp 480w, /media/${variant}-720.webp 720w, /media/${variant}-960.webp 960w`,
    width: 960,
    height: 960,
    sizes: posterSizes,
  };
}

function wide(variant: SceneVariant): MediaImage {
  return {
    avif: `/media/${variant}-wide-960.avif`,
    avifSet: `/media/${variant}-wide-960.avif 960w, /media/${variant}-wide-1440.avif 1440w`,
    webp: `/media/${variant}-wide-960.webp`,
    webpSet: `/media/${variant}-wide-960.webp 960w, /media/${variant}-wide-1440.webp 1440w`,
    width: 1440,
    height: 810,
    sizes: wideSizes,
  };
}

function cover(variant: SceneVariant): MediaImage {
  return {
    avif: `/media/${variant}-cover-720.avif`,
    avifSet: `/media/${variant}-cover-480.avif 480w, /media/${variant}-cover-720.avif 720w`,
    webp: `/media/${variant}-cover-720.webp`,
    webpSet: `/media/${variant}-cover-480.webp 480w, /media/${variant}-cover-720.webp 720w`,
    width: 720,
    height: 450,
    sizes: coverSizes,
  };
}

export const siteMedia = {
  poster: {
    island: poster("island"),
    cave: poster("cave"),
    portal: poster("portal"),
  },
  wide: {
    island: wide("island"),
    cave: wide("cave"),
    portal: wide("portal"),
  },
  cover: {
    island: cover("island"),
    cave: cover("cave"),
    portal: cover("portal"),
  },
  launcherPreview: {
    avif: "/media/launcher-preview-1180.avif",
    avifSet:
      "/media/launcher-preview-960.avif 960w, /media/launcher-preview-1180.avif 1180w",
    webp: "/media/launcher-preview-1180.webp",
    webpSet:
      "/media/launcher-preview-960.webp 960w, /media/launcher-preview-1180.webp 1180w",
    width: 1180,
    height: 760,
    sizes: previewSizes,
  },
  brand: "/brand-512.png",
  og: {
    home: "/og/home.jpg",
    features: "/og/features.jpg",
    download: "/og/download.jpg",
    news: "/og/news.jpg",
    article: {
      island: "/og/article-island.jpg",
      cave: "/og/article-cave.jpg",
      portal: "/og/article-portal.jpg",
    },
  },
};

export const mediaPreload: Record<string, PreloadImage> = {
  home: {
    href: siteMedia.poster.island.avif,
    type: "image/avif",
    srcSet: siteMedia.poster.island.avifSet,
    sizes: siteMedia.poster.island.sizes,
  },
  features: {
    href: siteMedia.wide.cave.avif,
    type: "image/avif",
    srcSet: siteMedia.wide.cave.avifSet,
    sizes: siteMedia.wide.cave.sizes,
  },
  download: {
    href: siteMedia.wide.portal.avif,
    type: "image/avif",
    srcSet: siteMedia.wide.portal.avifSet,
    sizes: siteMedia.wide.portal.sizes,
  },
  news: {
    href: siteMedia.cover.portal.avif,
    type: "image/avif",
    srcSet: siteMedia.cover.portal.avifSet,
    sizes: siteMedia.cover.portal.sizes,
  },
  "article-island": {
    href: siteMedia.wide.island.avif,
    type: "image/avif",
    srcSet: siteMedia.wide.island.avifSet,
    sizes: siteMedia.wide.island.sizes,
  },
  "article-cave": {
    href: siteMedia.wide.cave.avif,
    type: "image/avif",
    srcSet: siteMedia.wide.cave.avifSet,
    sizes: siteMedia.wide.cave.sizes,
  },
  "article-portal": {
    href: siteMedia.wide.portal.avif,
    type: "image/avif",
    srcSet: siteMedia.wide.portal.avifSet,
    sizes: siteMedia.wide.portal.sizes,
  },
};

export function ResponsiveImage({
  image,
  alt,
  className,
  imgClassName,
  priority = false,
  lazy = false,
  sizes,
}: {
  image: MediaImage;
  alt: string;
  className?: string;
  imgClassName?: string;
  priority?: boolean;
  lazy?: boolean;
  sizes?: string;
}) {
  const resolvedSizes = sizes ?? image.sizes;
  return (
    <picture className={`site-picture${className ? ` ${className}` : ""}`}>
      <source type="image/avif" srcSet={image.avifSet} sizes={resolvedSizes} />
      <source type="image/webp" srcSet={image.webpSet} sizes={resolvedSizes} />
      <img
        className={imgClassName}
        src={image.webp}
        srcSet={image.webpSet}
        sizes={resolvedSizes}
        width={image.width}
        height={image.height}
        alt={alt}
        loading={priority ? "eager" : lazy ? "lazy" : "eager"}
        fetchPriority={priority ? "high" : undefined}
        decoding="async"
      />
    </picture>
  );
}
