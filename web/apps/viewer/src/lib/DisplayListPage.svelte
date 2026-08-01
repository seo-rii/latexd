<script lang="ts">
  import type {
    BrowserFontFamily,
    BrowserPageDisplayList,
    BrowserSourceProvenance
  } from "./browser-artifacts";
  import {
    browserDestinationId,
    browserLinkHref,
    prepareDisplayListOps
  } from "./display-list-renderer";
  import { browserSourceKey } from "./display-list-source";

  let {
    page,
    pageNumber,
    assetUrls = {},
    activeSourceKey = null,
    onSourceHover,
    onSourceSelect
  } = $props<{
    page: BrowserPageDisplayList;
    pageNumber: number;
    assetUrls?: Record<string, string>;
    activeSourceKey?: string | null;
    onSourceHover?: (source: BrowserSourceProvenance | null) => void;
    onSourceSelect?: (source: BrowserSourceProvenance) => void;
  }>();

  const prepared = $derived(prepareDisplayListOps(page.ops));

  function clipId(index: number) {
    return `display-list-${page.page_id}-clip-${index}`;
  }

  function fontFamily(family: BrowserFontFamily) {
    if (typeof family === "object") {
      return family.named;
    }
    switch (family) {
      case "sans":
        return '"Source Sans 3", "Gill Sans", sans-serif';
      case "mono":
        return '"Iosevka", "IBM Plex Mono", monospace';
      case "math":
      case "symbol":
      case "math_extension":
        return '"STIX Two Math", "Latin Modern Math", "Cambria Math", serif';
      default:
        return '"Libertinus Serif", "Iowan Old Style", "Palatino Linotype", serif';
    }
  }

  function imageTransform(
    rect: { x: number; y: number; width: number; height: number },
    rotation?: { angle_degrees: number }
  ) {
    if (!rotation || rotation.angle_degrees === 0) {
      return undefined;
    }
    const centerX = rect.x + rect.width / 2;
    const centerY = rect.y + rect.height / 2;
    return `rotate(${rotation.angle_degrees} ${centerX} ${centerY})`;
  }

  function selectSource(
    event: MouseEvent,
    source: BrowserSourceProvenance,
    sourceKey: string | null
  ) {
    if (!sourceKey) {
      return;
    }
    event.preventDefault();
    onSourceSelect?.(source);
  }
</script>

<article
  class="display-list-page"
  aria-label={`Rendered page ${pageNumber}`}
  data-page-id={page.page_id}
  data-content-hash={page.content_hash}
  style={`aspect-ratio: ${page.width_pt} / ${page.height_pt}`}
>
  <svg
    role="img"
    aria-label={`Compiler display list page ${pageNumber}`}
    viewBox={`0 0 ${page.width_pt} ${page.height_pt}`}
    preserveAspectRatio="xMidYMid meet"
  >
    <defs>
      {#each prepared.ops as entry, index}
        {#if entry.clip_rect}
          <clipPath id={clipId(index)}>
            <rect
              x={entry.clip_rect.x}
              y={entry.clip_rect.y}
              width={entry.clip_rect.width}
              height={entry.clip_rect.height}
            />
          </clipPath>
        {/if}
      {/each}
    </defs>

    {#each prepared.ops as entry, index}
      {@const op = entry.op}
      {@const clipPath = entry.clip_rect ? `url(#${clipId(index)})` : undefined}
      {#if op.kind === "text_run"}
        {@const sourceKey = browserSourceKey(op.source)}
        <a
          href={sourceKey ? `#source-${encodeURIComponent(sourceKey)}` : undefined}
          aria-label={sourceKey ? `Select source for ${op.text}` : undefined}
          onpointerenter={() => onSourceHover?.(op.source)}
          onpointerleave={() => onSourceHover?.(null)}
          onclick={(event) => selectSource(event, op.source, sourceKey)}
        >
          <text
            x={op.origin.x}
            y={op.origin.y}
            font-family={fontFamily(op.font.family)}
            font-size={op.size_pt}
            font-weight={op.font.series === "bold" ? 700 : 400}
            font-style={op.font.shape === "italic" ? "italic" : "normal"}
            clip-path={clipPath}
            data-text-rendering="css-fallback"
            data-source-kind={op.source?.primary?.kind ?? "unknown"}
            data-source-key={sourceKey}
            class:display-list-source-linked={sourceKey !== null}
            class:display-list-source-active={sourceKey === activeSourceKey}
            xml:space="preserve"
          >{op.text}</text>
        </a>
      {:else if op.kind === "rule"}
        <rect
          x={op.x}
          y={op.y}
          width={op.width}
          height={op.height}
          clip-path={clipPath}
          fill="currentColor"
        />
      {:else if op.kind === "image"}
        {@const assetUrl = assetUrls[op.asset_ref]}
        {@const transform = imageTransform(op.rect, op.rotation)}
        {@const sourceKey = browserSourceKey(op.source)}
        <a
          href={sourceKey ? `#source-${encodeURIComponent(sourceKey)}` : undefined}
          aria-label={sourceKey ? `Select source for image ${op.asset_ref}` : undefined}
          onpointerenter={() => onSourceHover?.(op.source)}
          onpointerleave={() => onSourceHover?.(null)}
          onclick={(event) => selectSource(event, op.source, sourceKey)}
        >
          {#if assetUrl}
            <image
              href={assetUrl}
              x={op.rect.x}
              y={op.rect.y}
              width={op.rect.width}
              height={op.rect.height}
              preserveAspectRatio="none"
              transform={transform}
              clip-path={clipPath}
              data-source-key={sourceKey}
              class:display-list-source-linked={sourceKey !== null}
              class:display-list-source-active={sourceKey === activeSourceKey}
            />
          {:else}
            <g
              class="display-list-image-fallback"
              class:display-list-source-linked={sourceKey !== null}
              class:display-list-source-active={sourceKey === activeSourceKey}
              clip-path={clipPath}
              transform={transform}
              data-source-key={sourceKey}
            >
              <rect
                x={op.rect.x}
                y={op.rect.y}
                width={op.rect.width}
                height={op.rect.height}
                fill="#f5f0e7"
                stroke="#9b6f3f"
                stroke-dasharray="4 3"
              />
              <text
                x={op.rect.x + 6}
                y={op.rect.y + 16}
                font-size="9"
                fill="#704928"
              >{op.diagnostic ?? `[image: ${op.asset_ref}]`}</text>
            </g>
          {/if}
        </a>
      {:else if op.kind === "link_annotation"}
        {@const linkHref = browserLinkHref(op.target)}
        {@const sourceKey = browserSourceKey(op.source)}
        {#if linkHref}
          <a
            href={linkHref}
            aria-label={`Open ${op.target}`}
            data-source-key={sourceKey}
            class:display-list-source-active={sourceKey === activeSourceKey}
            onpointerenter={() => onSourceHover?.(op.source)}
            onpointerleave={() => onSourceHover?.(null)}
          >
            <rect
              x={op.rect.x}
              y={op.rect.y}
              width={op.rect.width}
              height={op.rect.height}
              clip-path={clipPath}
              fill="transparent"
              pointer-events="all"
            />
          </a>
        {:else}
          <a
            href={sourceKey ? `#source-${encodeURIComponent(sourceKey)}` : undefined}
            aria-label={sourceKey ? `Select source for blocked link ${op.target}` : undefined}
            onpointerenter={() => onSourceHover?.(op.source)}
            onpointerleave={() => onSourceHover?.(null)}
            onclick={(event) => selectSource(event, op.source, sourceKey)}
          >
            <rect
              class="display-list-link-blocked"
              class:display-list-source-linked={sourceKey !== null}
              class:display-list-source-active={sourceKey === activeSourceKey}
              x={op.rect.x}
              y={op.rect.y}
              width={op.rect.width}
              height={op.rect.height}
              clip-path={clipPath}
              fill="transparent"
              data-source-key={sourceKey}
              data-blocked-target={op.target}
            />
          </a>
        {/if}
      {:else if op.kind === "named_destination"}
        <g id={browserDestinationId(op.name)}>
          <circle cx={op.point.x} cy={op.point.y} r="0" />
        </g>
      {/if}
    {/each}
  </svg>

  <span class="display-list-page__number">{pageNumber}</span>
  {#if prepared.diagnostics.length > 0}
    <ul class="display-list-page__diagnostics" aria-label={`Page ${pageNumber} renderer diagnostics`}>
      {#each prepared.diagnostics as diagnostic}
        <li>{diagnostic}</li>
      {/each}
    </ul>
  {/if}
</article>

<style>
  .display-list-page {
    position: relative;
    box-sizing: border-box;
    width: min(100%, 52rem);
    margin: 0;
    overflow: hidden;
    color: #17130f;
    background:
      linear-gradient(90deg, rgba(96, 68, 39, 0.025) 1px, transparent 1px) 0 0 / 22px 22px,
      #fffef9;
    box-shadow: 0 18px 48px rgba(20, 16, 12, 0.25);
  }

  svg {
    display: block;
    width: 100%;
    height: 100%;
  }

  text[data-source-kind="file"]:not(.display-list-source-linked) {
    cursor: text;
  }

  .display-list-source-linked {
    cursor: pointer;
    transition: filter 120ms ease, opacity 120ms ease;
  }

  .display-list-source-linked:hover,
  a:focus-visible > .display-list-source-linked,
  .display-list-source-active {
    filter: drop-shadow(0 0 1.5px rgba(198, 84, 32, 0.85));
    opacity: 0.82;
    outline: none;
  }

  .display-list-page__number {
    position: absolute;
    right: 1rem;
    bottom: 0.7rem;
    padding: 0.2rem 0.45rem;
    border-radius: 999px;
    color: #5c4c3e;
    background: rgba(255, 254, 249, 0.88);
    font: 600 0.72rem/1 "Source Sans 3", sans-serif;
  }

  .display-list-page__diagnostics {
    position: absolute;
    right: 1rem;
    bottom: 2.25rem;
    max-width: calc(100% - 2rem);
    margin: 0;
    padding: 0.55rem 0.8rem 0.55rem 1.8rem;
    border: 1px solid #c58b52;
    border-radius: 0.45rem;
    color: #633817;
    background: rgba(255, 245, 228, 0.94);
    font: 0.72rem/1.35 "Source Sans 3", sans-serif;
  }
</style>
