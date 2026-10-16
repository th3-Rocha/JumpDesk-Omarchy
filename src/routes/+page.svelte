<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { api, type Bounds, type Config, type Status, type Quality } from "$lib/api";
  import StatusDot from "$lib/StatusDot.svelte";
  import QualityBars from "$lib/QualityBars.svelte";
  import Wizard from "$lib/Wizard.svelte";
  import ProfileEditor from "$lib/ProfileEditor.svelte";
  import Toasts from "$lib/Toasts.svelte";
  import LogView, { type LogLine } from "$lib/LogView.svelte";
  import Icon from "$lib/Icon.svelte";
  import { toast } from "$lib/toast.svelte";

  type View = "loading" | "wizard" | "panel" | "profiles";
  let view = $state<View>("loading");
  let tab = $state<"painel" | "logs">("painel");

  let config = $state<Config | null>(null);
  let status = $state<Status>({ vpn: "idle", tunnel: "idle", rdp: "idle" });
  let quality = $state<Quality>({ bars: 0, latency_ms: -1 });
  let logs = $state<LogLine[]>([]);
  let connectorOk = $state(true);
  let dlPct = $state<number | null>(null);
  let sessionReady = $state(false);
  let busy = $state("");
  // Área onde o portal (webview nativa) fica embutido; só existe na aba Painel.
  let portalSlot = $state<HTMLDivElement | null>(null);
  let portalOpen = $state(false);

  let logSeq = 0;
  function pushLog(line: string) {
    const m = /^\[([^\]]+)\]\s?([\s\S]*)$/.exec(line);
    const time = new Date().toLocaleTimeString("pt-BR", { hour12: false });
    const entry: LogLine = m
      ? { id: logSeq++, time, prefix: `[${m[1]}]`, msg: m[2] }
      : { id: logSeq++, time, prefix: "", msg: line };
    logs = [...logs.slice(-499), entry];
  }

  async function reload() {
    config = await api.getConfig();
    connectorOk = await api.connectorInstalled();
    status = await api.getStatus();
    view = config.configured ? "panel" : "wizard";
  }

  onMount(async () => {
    await reload();
    await listen<Status>("status", (e) => (status = e.payload));
    await listen<Quality>("quality", (e) => (quality = e.payload));
    await listen<string>("log", (e) => pushLog(e.payload));
    await listen<string>("notice", (e) => toast(e.payload));
    await listen<number>("connector-progress", (e) => {
      dlPct = e.payload;
      if (e.payload >= 100) setTimeout(() => (dlPct = null), 600);
    });
    await listen("session-captured", async () => {
      sessionReady = true;
      pushLog("[app] sessão capturada — subindo túnel e RDP…");
      toast("Sessão do portal capturada. Subindo túnel…", "info");
      await run("Subindo túnel e RDP", api.connectSession);
    });
  });

  function slotBounds(): Bounds | null {
    if (!portalSlot) return null;
    const r = portalSlot.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }

  async function openPortal() {
    // O slot só existe no Painel: volta para lá antes de medir.
    tab = "painel";
    view = "panel";
    await tick();
    const b = slotBounds();
    if (!b) throw "área do portal indisponível";
    await api.openPortal(b);
    portalOpen = true;
  }

  // Acompanha a área do portal: reposiciona quando ela muda de tamanho e
  // esconde a webview quando a área some (aba Logs, Perfis), senão a webview
  // nativa ficaria por cima dessas telas.
  $effect(() => {
    if (!portalOpen) return;
    const slot = portalSlot;
    if (!slot) {
      api.placePortal(null);
      return;
    }
    const place = () => api.placePortal(slotBounds());
    place();
    const ro = new ResizeObserver(place);
    ro.observe(slot);
    window.addEventListener("resize", place);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", place);
    };
  });

  async function run(label: string, fn: () => Promise<unknown>) {
    busy = label;
    try {
      await fn();
    } catch (e) {
      const err = String(e);
      pushLog("[erro] " + err);
      toast(err);
    } finally {
      busy = "";
    }
  }

  async function downloadConnector() {
    dlPct = 0;
    await run("Baixando VaultOneRDPConnector", api.connectorDownload);
    connectorOk = await api.connectorInstalled();
    if (connectorOk) toast("VaultOneRDPConnector instalado.", "info");
  }

  async function connectAll() {
    await run("Conectando VPN", api.vpnConnect);
    await run("Abrindo portal", openPortal);
    pushLog("[app] portal aberto — faça login (ou aguarde o preenchimento) e clique no servidor.");
  }

  async function cleanup() {
    await run("Limpando", api.cleanup);
    sessionReady = false;
  }

  async function backToPanel(to: "painel" | "logs" = "painel") {
    config = await api.getConfig();
    view = "panel";
    tab = to;
  }
</script>

<Toasts />

{#snippet topbar()}
  <header class="jd-topbar">
    <div class="jd-brand"><strong>JumpDesk</strong></div>
    <div class="jd-tabs" role="tablist">
      <button class="jd-tab" role="tab" aria-selected={view === "panel" && tab === "painel"}
        onclick={() => backToPanel("painel")}>Painel</button>
      <button class="jd-tab" role="tab" aria-selected={view === "panel" && tab === "logs"}
        onclick={() => backToPanel("logs")}>Logs{#if logs.length}<span class="count">{logs.length}</span>{/if}</button>
    </div>
    <div class="jd-spacer"></div>
    <QualityBars bars={quality.bars} latency={quality.latency_ms} />
    <button class="jd-btn jd-btn--ghost" aria-pressed={view === "profiles"}
      onclick={() => (view = "profiles")}><Icon name="sliders" />Perfis RDP</button>
  </header>
{/snippet}

{#if view === "loading"}
  <div class="center">carregando…</div>
{:else if view === "wizard" && config}
  <Wizard {config} done={reload} />
{:else if view === "profiles" && config}
  <div class="app">
    {@render topbar()}
    <ProfileEditor {config} back={() => backToPanel()} />
  </div>
{:else if config}
  <div class="app">
  {@render topbar()}

  {#if tab === "painel"}
    <div class="jd-strip-main">
      {#if !connectorOk}
        <div class="jd-banner">
          {#if dlPct === null}
            <p><strong>VaultOneRDPConnector não instalado.</strong> Sem ele o túnel não sobe.</p>
            <button class="jd-btn" onclick={downloadConnector}><Icon name="download" />Baixar agora</button>
          {:else}
            <p><strong>Baixando VaultOneRDPConnector…</strong></p>
            <div class="jd-progress"><div><i style="width:{dlPct}%"></i></div>{dlPct}%</div>
          {/if}
        </div>
      {/if}

      <div class="jd-chain">
        <StatusDot label="VPN" stage={status.vpn} /><i></i>
        <StatusDot label="Túnel" stage={status.tunnel} /><i></i>
        <StatusDot label="RDP" stage={status.rdp} />
      </div>

      <div class="jd-row">
        <button class="jd-btn jd-btn--primary" disabled={!!busy} onclick={connectAll}>
          <Icon name="play" />Conectar tudo</button>
        <button class="jd-btn" disabled={!!busy} onclick={() => run("Conectando VPN", api.vpnConnect)}>VPN</button>
        <button class="jd-btn" disabled={!!busy} onclick={() => run("Abrindo portal", openPortal)}>Portal</button>
        <button class="jd-btn" disabled={!!busy || !sessionReady}
          title={sessionReady ? "" : "Libera quando você clicar no servidor no portal"}
          onclick={() => run("Subindo túnel e RDP", api.connectSession)}>Túnel + RDP</button>
        <span class="jd-btn-sep"></span>
        <button class="jd-btn jd-btn--danger" disabled={!!busy}
          onclick={() => run("Desconectando", api.disconnectAll)}><Icon name="square" />Desconectar</button>
        <button class="jd-btn jd-btn--ghost" disabled={!!busy}
          title="Mata VPN, túnel e RDP que tenham sobrado (inclusive abertos fora do app) e libera a porta 3390"
          onclick={cleanup}><Icon name="eraser" />Limpar</button>
      </div>

      {#if busy}
        <div class="jd-strip-foot busy"><span class="jd-dot" data-s="busy"></span>{busy}…</div>
      {:else}
        <div class="jd-strip-foot">
          Depois de conectar, clique no servidor no portal: o túnel e o RDP sobem sozinhos.
        </div>
      {/if}
    </div>

    <div class="portal-slot" class:open={portalOpen} bind:this={portalSlot}>
      {#if !portalOpen}<div class="jd-portal">O portal do JumpCloud abre aqui.</div>{/if}
    </div>
  {:else}
    <div class="logs"><LogView lines={logs} /></div>
  {/if}
  </div>
{/if}

<style>
  .center { padding: 3rem; text-align: center; color: var(--muted); }
  .app { display: flex; flex-direction: column; height: 100vh; }
  .portal-slot { flex: 1; min-height: 160px; display: flex; flex-direction: column;
    margin: 0 var(--space-4) var(--space-4); }
  .portal-slot .jd-portal { flex: 1; }
  /* aberto, o portal fica logo abaixo da página (no Linux a webview dele é
     empilhada pelo GTK, não sobreposta): a área vira só o marcador do topo */
  .portal-slot.open { flex: 0; min-height: 0; margin: 0; }
  .logs { flex: 1; min-height: 0; display: flex; padding: var(--space-3) var(--space-4) var(--space-4); }
</style>
