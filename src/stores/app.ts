// Zustand store for global app state
// Note: Install zustand when setting up the project: npm install zustand

import { create } from 'zustand';
import type { UserSession, AppConfig, ChannelState, AudioMetrics } from '../ipc/types';

interface AppState {
  // Auth state
  session: UserSession | null;
  hasByokKey: boolean;
  
  // Audio state
  systemChannelState: ChannelState;
  userChannelState: ChannelState;
  audioMetrics: AudioMetrics | null;
  
  // UI state
  isOnboarding: boolean;
  showSettings: boolean;
  
  // Config
  config: AppConfig | null;
  
  // Actions
  setSession: (session: UserSession | null) => void;
  setHasByokKey: (has: boolean) => void;
  setSystemChannelState: (state: ChannelState) => void;
  setUserChannelState: (state: ChannelState) => void;
  setAudioMetrics: (metrics: AudioMetrics | null) => void;
  setIsOnboarding: (isOnboarding: boolean) => void;
  setShowSettings: (show: boolean) => void;
  setConfig: (config: AppConfig | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  // Initial state
  session: null,
  hasByokKey: false,
  systemChannelState: { type: 'inactive' },
  userChannelState: { type: 'inactive' },
  audioMetrics: null,
  isOnboarding: false,
  showSettings: false,
  config: null,
  
  // Actions
  setSession: (session) => set({ session }),
  setHasByokKey: (hasByokKey) => set({ hasByokKey }),
  setSystemChannelState: (systemChannelState) => set({ systemChannelState }),
  setUserChannelState: (userChannelState) => set({ userChannelState }),
  setAudioMetrics: (audioMetrics) => set({ audioMetrics }),
  setIsOnboarding: (isOnboarding) => set({ isOnboarding }),
  setShowSettings: (showSettings) => set({ showSettings }),
  setConfig: (config) => set({ config }),
}));
