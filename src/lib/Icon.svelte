<script lang="ts">
  // Ícones Lucide usados no app (traço 2, 14px, currentColor).
  export type IconName =
    | "play" | "square" | "eraser" | "sliders" | "download" | "check"
    | "qr" | "copy" | "arrow-left" | "arrow-right" | "plus" | "folder";
  let { name }: { name: IconName } = $props();
</script>

<svg class="i" viewBox="0 0 24 24" aria-hidden="true">
  {#if name === "play"}<polygon points="6 3 20 12 6 21 6 3" />
  {:else if name === "square"}<rect x="5" y="5" width="14" height="14" rx="2" />
  {:else if name === "eraser"}<path d="m7 21-4.3-4.3c-1-1-1-2.5 0-3.4l9.6-9.6c1-1 2.5-1 3.4 0l5.6 5.6c1 1 1 2.5 0 3.4L13 21" /><path d="M22 21H7" /><path d="m5 11 9 9" />
  {:else if name === "sliders"}<path d="M20 7h-9" /><path d="M14 17H5" /><circle cx="17" cy="17" r="3" /><circle cx="7" cy="7" r="3" />
  {:else if name === "download"}<path d="M12 15V3" /><path d="m7 10 5 5 5-5" /><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
  {:else if name === "check"}<path d="M20 6 9 17l-5-5" />
  {:else if name === "qr"}<rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><path d="M14 14h3v3h-3zM20 20h1M17 20h-3" />
  {:else if name === "copy"}<rect x="8" y="8" width="14" height="14" rx="2" /><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
  {:else if name === "arrow-left"}<path d="m12 19-7-7 7-7" /><path d="M19 12H5" />
  {:else if name === "arrow-right"}<path d="M5 12h14" /><path d="m12 5 7 7-7 7" />
  {:else if name === "plus"}<path d="M5 12h14" /><path d="M12 5v14" />
  {:else if name === "folder"}<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" />
  {/if}
</svg>
