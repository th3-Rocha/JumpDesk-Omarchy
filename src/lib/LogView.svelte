<script lang="ts" module>
  export type LogLine = { id: number; time: string; prefix: string; msg: string };
</script>

<script lang="ts">
  import { tick } from "svelte";

  let { lines }: { lines: LogLine[] } = $props();
  let box = $state<HTMLDivElement | null>(null);
  // segue o fim, a menos que o usuário tenha rolado para cima
  let follow = true;

  function onScroll() {
    if (!box) return;
    follow = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
  }

  $effect(() => {
    lines.length;
    if (follow) tick().then(() => box && (box.scrollTop = box.scrollHeight));
  });

  function kind(prefix: string): string {
    if (prefix === "[erro]") return "erro";
    if (prefix === "[aviso]") return "aviso";
    if (prefix === "[app]") return "app";
    return "";
  }
</script>

<div class="jd-log fill" bind:this={box} onscroll={onScroll}>
  {#each lines as l (l.id)}
    {@const k = kind(l.prefix)}
    <div class:is-erro={k === "erro"}>
      <time>{l.time}</time><span class="p {k ? `p--${k}` : ''}">{l.prefix}</span>{l.msg}
    </div>
  {/each}
  {#if lines.length === 0}<div><time></time><span></span><span class="empty">sem logs ainda.</span></div>{/if}
</div>

<style>
  .fill { flex: 1; min-height: 0; }
  .empty { color: var(--muted); }
</style>
