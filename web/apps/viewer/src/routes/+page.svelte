<script lang="ts">
  import { mountLatexdViewerHost } from "$lib";

  const mountLatexdViewer = (node: HTMLDivElement) => {
    const viewer = mountLatexdViewerHost(node);
    return () => {
      viewer.destroy();
    };
  };
</script>

<svelte:head>
  <title>latexd viewer</title>
</svelte:head>

<div class="viewer-root" {@attach mountLatexdViewer}></div>

<style>
  .viewer-root {
    min-height: 100vh;
  }
</style>
