<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type Config, type ImportedAccount } from "./api";
  import { toast } from "./toast.svelte";
  import Icon from "./Icon.svelte";

  let { config, done }: { config: Config; done: () => void } = $props();

  let step = $state(1);
  let cfg = $state<Config>(structuredClone($state.snapshot(config)));

  // segredos (ficam só na memória até salvar no chaveiro)
  let vpnPassword = $state("");
  let vpnSeed = $state("");
  let jumpPassword = $state("");
  let jumpSeed = $state("");
  let windowsPassword = $state("");

  // contas importadas do Google Authenticator (QR ou texto migration)
  let accounts = $state<ImportedAccount[]>([]);
  let importText = $state("");

  let busy = $state(false);

  async function pickOvpn() {
    const f = await open({ filters: [{ name: "OpenVPN", extensions: ["ovpn"] }] });
    if (typeof f === "string") cfg.vpn.ovpn_path = f;
  }

  function mergeAccounts(found: ImportedAccount[]) {
    const seen = new Set(accounts.map((a) => a.secret_base32));
    accounts = [...accounts, ...found.filter((a) => !seen.has(a.secret_base32))];
    if (accounts.length)
      toast(`${accounts.length} conta(s) importada(s). Escolha abaixo qual é a VPN e qual é o JumpCloud.`, "info");
    else toast("nenhuma conta encontrada.");
  }

  // lê o QR LOCALMENTE (offline), sem sites de terceiros
  async function importFromImage() {
    const f = await open({ filters: [{ name: "Imagem", extensions: ["png", "jpg", "jpeg"] }] });
    if (typeof f !== "string") return;
    try {
      mergeAccounts(await api.importSeedImage(f));
    } catch (e) {
      toast(String(e));
    }
  }

  // aceita texto colado: base32, otpauth:// ou otpauth-migration://
  async function importFromText() {
    if (!importText.trim()) return;
    try {
      mergeAccounts(await api.importSeedText(importText.trim()));
      importText = "";
    } catch (e) {
      toast(String(e));
    }
  }

  function label(a: ImportedAccount): string {
    return [a.issuer, a.name].filter(Boolean).join(" · ") || "(sem nome)";
  }

  async function finish() {
    if (!vpnPassword || !jumpPassword) {
      toast("Preencha a senha da VPN e a do JumpCloud.");
      return;
    }
    if (!/^https:\/\/\S+$/.test(cfg.jumpcloud.portal_url.trim())) {
      toast("Preencha a URL do portal do JumpCloud (passo 2), começando com https://.");
      return;
    }
    cfg.jumpcloud.portal_url = cfg.jumpcloud.portal_url.trim();
    busy = true;
    try {
      await api.saveSetup({
        config: $state.snapshot(cfg),
        vpn_password: vpnPassword,
        vpn_seed: vpnSeed,
        jump_password: jumpPassword,
        jump_seed: jumpSeed,
        windows_password: windowsPassword || null,
      });
      done();
    } catch (e) {
      toast(String(e));
    } finally {
      busy = false;
    }
  }

  const steps = ["VPN", "JumpCloud", "Máquina Windows"];
</script>

{#snippet seedPicker(set: (v: string) => void)}
  {#if accounts.length}
    <div class="jd-field">
      <label for="seed-pick">…ou escolha da importação</label>
      <select id="seed-pick" class="jd-select" onchange={(e) => set(e.currentTarget.value)}>
        <option value="">— selecionar —</option>
        {#each accounts as a}<option value={a.secret_base32}>{label(a)}</option>{/each}
      </select>
    </div>
  {/if}
{/snippet}

{#snippet importBox()}
  <div class="jd-box import">
    <p class="jd-box-h">Importar do Google Authenticator</p>
    <div class="import-grid">
      <button type="button" class="jd-btn" onclick={importFromImage}><Icon name="qr" />Ler QR de imagem…</button>
      <div class="jd-field">
        <textarea class="jd-textarea" bind:value={importText} rows="2"
          placeholder="otpauth-migration://offline?data=… (ou otpauth://…)"></textarea>
        <div><button type="button" class="jd-btn" disabled={!importText.trim()}
          onclick={importFromText}>Processar texto colado</button></div>
      </div>
    </div>
    <span class="hint">Lido localmente, offline: nada sai da sua máquina.</span>
  </div>
{/snippet}

<div class="wizard">
  <h2 class="title">Configuração inicial — passo {step} de 3</h2>
  <div class="jd-steps">
    {#each steps as name, i}
      {#if i > 0}<i></i>{/if}
      <div class="jd-step" data-s={i + 1 < step ? "done" : i + 1 === step ? "current" : undefined}>
        <b>{i + 1}</b>{name}
      </div>
    {/each}
  </div>

  <div class="jd-box">
    {#if step === 1}
      <p class="jd-box-h">VPN</p>
      <div class="stack">
        <div class="jd-field">
          <label for="ovpn">Arquivo .ovpn</label>
          <div class="jd-combo">
            <input id="ovpn" class="jd-input" readonly value={cfg.vpn.ovpn_path} placeholder="nenhum selecionado" />
            <button class="jd-btn" onclick={pickOvpn}><Icon name="folder" />Escolher…</button>
          </div>
        </div>
        <div class="jd-field"><label for="vpn-user">Usuário</label>
          <input id="vpn-user" class="jd-input" bind:value={cfg.vpn.username} /></div>
        <div class="jd-field"><label for="vpn-pass">Senha</label>
          <input id="vpn-pass" class="jd-input" type="password" bind:value={vpnPassword} /></div>
        <div class="jd-field"><label for="vpn-seed">Seed do MFA (VPN)</label>
          <input id="vpn-seed" class="jd-input" bind:value={vpnSeed} placeholder="cole o base32 aqui, ou importe abaixo" /></div>
        {@render seedPicker((v) => (vpnSeed = v))}
        {@render importBox()}
      </div>
    {:else if step === 2}
      <p class="jd-box-h">JumpCloud</p>
      <div class="stack">
        <div class="jd-field"><label for="jc-url">URL do portal</label>
          <input id="jc-url" class="jd-input" type="url" bind:value={cfg.jumpcloud.portal_url}
            placeholder="https://<empresa>.vault.jumpcloud.com/app/main/resources/servers" />
          <span class="hint">A página de servidores do JumpCloud PAM da sua empresa. Peça a quem te passou o app.</span></div>
        <div class="jd-field"><label for="jc-mail">E-mail</label>
          <input id="jc-mail" class="jd-input" type="email" bind:value={cfg.jumpcloud.email} /></div>
        <div class="jd-field"><label for="jc-pass">Senha</label>
          <input id="jc-pass" class="jd-input" type="password" bind:value={jumpPassword} /></div>
        <div class="jd-field"><label for="jc-seed">Seed do MFA (JumpCloud)</label>
          <input id="jc-seed" class="jd-input" bind:value={jumpSeed} placeholder="cole o base32 aqui, ou importe abaixo" /></div>
        {@render seedPicker((v) => (jumpSeed = v))}
        {@render importBox()}
      </div>
    {:else}
      <p class="jd-box-h">Máquina Windows</p>
      <div class="stack">
        <div class="jd-field"><label for="w-dom">Domínio</label>
          <input id="w-dom" class="jd-input" bind:value={cfg.rdp_profiles[0].domain} placeholder="ex. matriz.empresa.local" /></div>
        <div class="jd-field"><label for="w-user">Usuário</label>
          <input id="w-user" class="jd-input" bind:value={cfg.rdp_profiles[0].username} /></div>
        <div class="jd-field"><label for="w-pass">Senha do Windows</label>
          <input id="w-pass" class="jd-input" type="password" placeholder="opcional" bind:value={windowsPassword} />
          <span class="hint">Em branco, você digita na tela da VM.</span></div>
      </div>
    {/if}

    <div class="nav">
      {#if step > 1}<button class="jd-btn" onclick={() => (step -= 1)}><Icon name="arrow-left" />Voltar</button>{/if}
      <span class="jd-spacer"></span>
      {#if step < 3}
        <button class="jd-btn jd-btn--primary" onclick={() => (step += 1)}>Próximo<Icon name="arrow-right" /></button>
      {:else}
        <button class="jd-btn jd-btn--primary" disabled={busy} onclick={finish}>
          {busy ? "Salvando…" : "Concluir"}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .wizard { padding: var(--space-6); max-width: 640px; display: flex; flex-direction: column; gap: var(--space-4); }
  .title { margin: 0; font: 600 15px/20px var(--font-sans); }
  .stack { display: flex; flex-direction: column; gap: var(--space-3); }
  .import { padding: var(--space-3); background: var(--bg); display: flex; flex-direction: column; gap: var(--space-2); }
  .import .jd-box-h { margin: 0 0 var(--space-1); }
  .import-grid { display: grid; grid-template-columns: auto 1fr; gap: var(--space-3); align-items: start; }
  .import .jd-field > div { display: flex; }
  .import .hint { font-size: 12px; line-height: 16px; color: var(--muted); }
  .nav { display: flex; align-items: center; margin-top: var(--space-4); padding-top: var(--space-4);
    border-top: 1px solid var(--border); }
</style>
