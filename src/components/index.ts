/**
 * Components Index
 * 
 * Central export point for all UI components.
 * Components are adapted from the web project to use Tauri IPC.
 * 
 * @module components
 * @see Requirement 12.1 - Reuse components from web project
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 */

export { Header } from './Header';
export { SystemAudioPanel } from './SystemAudioPanel';
export { UserMicPanel } from './UserMicPanel';
export { SettingsModal } from './SettingsModal';
export { 
  LanguageSelector, 
  DualLanguageSelector, 
  LanguageSwapButton,
  SUPPORTED_LANGUAGES,
  getLanguageByCode,
  getLanguageDisplayName,
} from './LanguageSelector';
export type { Language } from './LanguageSelector';
export { AudioDeviceSelector, AudioDeviceSelectorGroup } from './AudioDeviceSelector';
export { OnboardingWizard } from './OnboardingWizard';
export type { OnboardingWizardProps } from './OnboardingWizard';
export { AudioTestPanel } from './AudioTestPanel';
export type { AudioTestPanelProps } from './AudioTestPanel';
export { UsageDashboard } from './UsageDashboard';
export type { UsageDashboardProps } from './UsageDashboard';
export { SubscriptionPage } from './SubscriptionPage';
export type { default as SubscriptionPageDefault } from './SubscriptionPage';
