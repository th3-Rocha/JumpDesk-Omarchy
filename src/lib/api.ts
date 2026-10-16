import { invoke } from "@tauri-apps/api/core";

export type Stage = "idle" | "connecting" | "up" | "error";
export type Status = { vpn: Stage; tunnel: Stage; rdp: Stage };
export type Quality = { bars: number; latency_ms: number };

export type RdpProfile = {
  name: string;
  domain: string;
  username: string;
  window_mode: "fullscreen" | "fixed" | "dynamic";
  width: number;
  height: number;
  monitor_mode: "single" | "all" | "list";
  monitor_ids: string;
  quality: "fast" | "balanced" | "max";
  clipboard: boolean;
  sound: boolean;
  microphone: boolean;
  shared_folder: string;
  extra_flags: string;
  workspace: string;
};

export type Config = {
  configured: boolean;
  vpn: { ovpn_path: string; username: string; trust_server: boolean; routes: string[]; tun_mtu: number; mssfix: number };
  jumpcloud: { email: string; portal_url: string };
  rdp_profiles: RdpProfile[];
  active_profile: number;
  portal_user_agent: string;
};

export type ImportedAccount = { name: string; issuer: string; secret_base32: string };

/** Área da janela (px lógicos) onde o portal fica embutido. */
export type Bounds = { x: number; y: number; width: number; height: number };

export const api = {
  getConfig: () => invoke<Config>("get_config"),
  isConfigured: () => invoke<boolean>("is_configured"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  hasVpnPassword: () => invoke<boolean>("has_vpn_password"),
  setVpnPassword: (password: string) => invoke<void>("set_vpn_password", { password }),
  saveSetup: (input: {
    config: Config;
    vpn_password: string;
    vpn_seed: string;
    jump_password: string;
    jump_seed: string;
    windows_password: string | null;
  }) => invoke<void>("save_setup", { input }),
  importSeedText: (text: string) => invoke<ImportedAccount[]>("import_seed_text", { text }),
  importSeedImage: (path: string) => invoke<ImportedAccount[]>("import_seed_image", { path }),

  getStatus: () => invoke<Status>("get_status"),
  hasWindowsPassword: () => invoke<boolean>("has_windows_password"),

  connectorInstalled: () => invoke<boolean>("connector_installed"),
  connectorDownload: () => invoke<void>("connector_download"),

  vpnConnect: () => invoke<void>("vpn_connect"),
  vpnDisconnect: () => invoke<void>("vpn_disconnect"),
  openPortal: (bounds: Bounds) => invoke<void>("open_portal", { bounds }),
  placePortal: (bounds: Bounds | null) => invoke<void>("place_portal", { bounds }),
  connectSession: () => invoke<void>("connect_session"),
  openRdp: () => invoke<void>("open_rdp"),
  disconnectAll: () => invoke<void>("disconnect_all"),
  cleanup: () => invoke<void>("cleanup"),
  rdpPreview: () => invoke<string>("rdp_preview"),
  hyprWorkspaces: () => invoke<string[]>("hypr_workspaces"),
};
