<script lang="ts">
  import { api, type Config } from "./api";
  import { toast } from "./toast.svelte";
  import Icon from "./Icon.svelte";

  let { config, back }: { config: Config; back: () => void } = $props();
  let cfg = $state<Config>(structuredClone($state.snapshot(config)));
  let idx = $state(config.active_profile ?? 0);
  let preview = $state("");
  let saved = $state(false);
  let vpnPass = $state("");
  let vpnPassMsg = $state("");
  let hasVpnPass = $state(false);
  api.hasVpnPassword().then((v) => (hasVpnPass = v));
  // vazio = não está no Hyprland: o seletor nem aparece
  let workspaces = $state<string[]>([]);
  api.hyprWorkspaces().then((w) => (workspaces = w));
  function wsLabel(w: string): string {
    if (w === "special:scratchpad") return "Scratchpad (Super+S)";
    if (w.startsWith("special:")) return `Especial: ${w.slice(8)}`;
    return `Workspace ${w}`;
  }

  async function saveVpnPass() {
    try {
      await api.setVpnPassword(vpnPass);
      vpnPass = "";
      hasVpnPass = await api.hasVpnPassword();
      vpnPassMsg = hasVpnPass ? "Senha salva ✓" : "";
      if (!hasVpnPass) toast("não consegui gravar a senha no chaveiro");
    } catch (e) {
      vpnPassMsg = "";
      toast(String(e));
    }
  }

  let p = $derived(cfg.rdp_profiles[idx]);

  async function refreshPreview() {
    cfg.active_profile = idx;
    await api.saveConfig($state.snapshot(cfg));
    preview = await api.rdpPreview();
  }

  async function save() {
    try {
      if (vpnPass) await saveVpnPass();
      cfg.active_profile = idx;
      await api.saveConfig($state.snapshot(cfg));
      preview = await api.rdpPreview();
      saved = true;
      setTimeout(() => (saved = false), 2000);
    } catch (e) {
      toast(String(e));
    }
  }

  function addProfile() {
    const base = structuredClone($state.snapshot(cfg.rdp_profiles[idx]));
    base.name = base.name + " (cópia)";
    cfg.rdp_profiles = [...cfg.rdp_profiles, base];
    idx = cfg.rdp_profiles.length - 1;
  }

  $effect(() => {
    refreshPreview();
  });

  // "Comando gerado": flags em destaque, a senha nunca aparece
  type Part = { flag: string; value: string; secret: boolean };
  let parts = $derived<Part[]>(
    preview.split(/\s+/).filter(Boolean).slice(1).map((tok) => {
      const m = /^([/+-][^:]*:?)(.*)$/.exec(tok);
      const flag = m ? m[1] : "";
      const value = m ? m[2] : tok;
      return { flag, value, secret: flag === "/p:" };
    }),
  );
  let copied = $state(false);
  async function copyCmd() {
    try {
      await navigator.clipboard.writeText(preview);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch (e) {
      toast("Não consegui copiar o comando: " + String(e));
    }
  }

  const windowModes = [["fixed", "Tamanho fixo"], ["fullscreen", "Tela cheia"], ["dynamic", "Dinâmica"]] as const;
  const monitorModes = [["single", "Um"], ["all", "Todos"], ["list", "Alguns"]] as const;
  const qualities = [["fast", "Rápido"], ["balanced", "Equilibrado"], ["max", "Máxima"]] as const;
</script>

{#snippet seg(options: readonly (readonly [string, string])[], value: string, set: (v: string) => void)}
  <div class="jd-seg" role="group">
    {#each options as [v, text]}
      <button type="button" aria-pressed={value === v} onclick={() => set(v)}>{text}</button>
    {/each}
  </div>
{/snippet}

<div class="editor">
  <div class="jd-row top">
    <button class="jd-btn" onclick={back}><Icon name="arrow-left" />Painel</button>
    <select class="jd-select profile" bind:value={idx} aria-label="Perfil">
      {#each cfg.rdp_profiles as prof, i}<option value={i}>{prof.name}</option>{/each}
    </select>
    <button class="jd-btn" onclick={addProfile}><Icon name="plus" />Perfil</button>
    <span class="jd-spacer"></span>
    <button class="jd-btn jd-btn--primary" onclick={save}>
      {#if saved}<Icon name="check" />Salvo{:else}Salvar{/if}
    </button>
  </div>

  <div class="jd-box">
    <p class="jd-box-h">VPN</p>
    <div class="stack">
      <div>
        <label class="jd-check">
          <input type="checkbox" bind:checked={cfg.vpn.trust_server} />
          Confiar no servidor da VPN (recomendado)
        </label>
        <p class="hint">
          {#if cfg.vpn.trust_server}
            Aceita as rotas e o DNS que a empresa manda — se mudarem algo lá, você
            pega sozinho, sem adicionar rota na mão.
          {:else}
            Modo manual: ignora o servidor, usa a lista de rotas fixa e bloqueia o
            DNS da empresa. Só use se souber o que está fazendo.
          {/if}
        </p>
      </div>
      <div class="jd-field">
        <label for="vpn-pass">Senha da VPN</label>
        <div class="jd-combo">
          <input id="vpn-pass" class="jd-input" type="password" bind:value={vpnPass} placeholder={hasVpnPass
              ? "•••••••• (salva — digite para trocar)"
              : "sem o código do MFA — ainda não configurada"} />
          <button class="jd-btn" disabled={!vpnPass} onclick={saveVpnPass}>Salvar senha</button>
        </div>
        {#if vpnPassMsg}<span class="hint ok">{vpnPassMsg}</span>{/if}
      </div>
    </div>
  </div>

  <div class="jd-box">
    <p class="jd-box-h">Perfil</p>
    <div class="jd-grid">
      <div class="jd-field"><label for="p-name">Nome</label><input id="p-name" class="jd-input" bind:value={p.name} /></div>
      <div class="jd-field"><label for="p-dom">Domínio</label><input id="p-dom" class="jd-input" bind:value={p.domain} /></div>
      <div class="jd-field"><label for="p-user">Usuário</label><input id="p-user" class="jd-input" bind:value={p.username} /></div>
    </div>

    <div class="opts">
      <span class="jd-box-h">Janela</span>
      <div class="jd-row">
        {@render seg(windowModes, p.window_mode, (v) => (p.window_mode = v as typeof p.window_mode))}
        {#if p.window_mode === "fixed"}
          <input class="jd-input num" type="number" aria-label="Largura" bind:value={p.width} />
          <span class="x">×</span>
          <input class="jd-input num" type="number" aria-label="Altura" bind:value={p.height} />
        {/if}
      </div>

      <span class="jd-box-h">Monitores</span>
      <div class="jd-row">
        {@render seg(monitorModes, p.monitor_mode, (v) => (p.monitor_mode = v as typeof p.monitor_mode))}
        {#if p.monitor_mode === "list"}
          <input class="jd-input ids" aria-label="IDs dos monitores" placeholder="IDs, ex. 0,1" bind:value={p.monitor_ids} />
        {/if}
      </div>

      {#if workspaces.length}
        <span class="jd-box-h">Workspace</span>
        <div class="jd-row">
          <select class="jd-select ws" aria-label="Workspace do Hyprland" bind:value={p.workspace}>
            <option value="">Onde o Hyprland abrir</option>
            {#if p.workspace && !workspaces.includes(p.workspace)}
              <option value={p.workspace}>{wsLabel(p.workspace)}</option>
            {/if}
            {#each workspaces as w}<option value={w}>{wsLabel(w)}</option>{/each}
          </select>
          <span class="hint inline">A janela do RDP vai para lá assim que abre, e você vai junto.</span>
        </div>
      {/if}

      <span class="jd-box-h">Qualidade</span>
      <div>{@render seg(qualities, p.quality, (v) => (p.quality = v as typeof p.quality))}</div>

      <span class="jd-box-h">Recursos</span>
      <div class="jd-row checks">
        <label class="jd-check"><input type="checkbox" bind:checked={p.clipboard} />Clipboard</label>
        <label class="jd-check"><input type="checkbox" bind:checked={p.sound} />Som</label>
        <label class="jd-check"><input type="checkbox" bind:checked={p.microphone} />Microfone</label>
      </div>
    </div>

    <div class="jd-grid">
      <div class="jd-field"><label for="p-share">Pasta compartilhada</label>
        <input id="p-share" class="jd-input" bind:value={p.shared_folder} placeholder="/caminho (opcional)" /></div>
      <div class="jd-field"><label for="p-flags">Flags avançadas</label>
        <input id="p-flags" class="jd-input" bind:value={p.extra_flags} placeholder="ex. /scale:140" /></div>
    </div>
  </div>

  <div class="jd-field">
    <span class="lbl">Comando gerado</span>
    <pre class="jd-cmd">xfreerdp3 {#each parts as part}{#if part.secret}<span class="s">/p:{part.value === "-" ? "-" : "••••••"}</span>{:else}{#if part.flag}<span class="f">{part.flag}</span>{/if}{part.value}{/if}{" "}{/each}<button class="jd-btn" onclick={copyCmd}><Icon name={copied ? "check" : "copy"} />{copied ? "Copiado" : "Copiar"}</button></pre>
  </div>
</div>

<style>
  .editor { padding: var(--space-6); display: flex; flex-direction: column; gap: var(--space-4);
    overflow: auto; flex: 1; min-height: 0; }
  .top .profile { width: 240px; }
  .stack { display: flex; flex-direction: column; gap: var(--space-3); }
  .hint { margin: var(--space-1) 0 0; font-size: 12px; line-height: 16px; color: var(--muted); }
  .hint.ok { color: var(--accent-strong); }
  .opts { display: grid; grid-template-columns: max-content 1fr; gap: var(--space-3) var(--space-4);
    align-items: center; margin: var(--space-4) 0; }
  .opts .jd-box-h { margin: 0; }
  .num { width: 90px; font-family: var(--font-mono); }
  .ids { width: 160px; }
  .ws { width: 240px; }
  .hint.inline { margin: 0; }
  .x { color: var(--muted); }
  .checks { gap: var(--space-4); }
</style>
