/**
 * Language Selector Component
 * 
 * Component for selecting translation target languages for each audio channel.
 * Supports languages available in Gemini Live translate preview and persists
 * selections between sessions.
 * 
 * @module components/LanguageSelector
 * @see Requirement 15.1 - Select target language for Audio_Channel_System
 * @see Requirement 15.2 - Select target language for Audio_Channel_User
 * @see Requirement 15.3 - Support languages available in Gemini Live translate preview
 * @see Requirement 15.4 - Persist language selection between sessions
 * @see Requirement 15.5 - Update Gemini configuration for next session on change
 */

import { useState, useCallback, useEffect } from 'react';
import { useConfig } from '../hooks/useConfig';
import type { ChannelType } from '../ipc/types';

/**
 * Languages supported by Gemini Live translate preview
 * Based on Google's Gemini Live translation capabilities
 * @see https://ai.google.dev/gemini-api/docs/live
 */
export interface Language {
  /** ISO 639-1 language code */
  code: string;
  /** Display name in Spanish (user's locale) */
  name: string;
  /** Display name in the language itself (native name) */
  nativeName: string;
  /** Flag emoji for visual identification */
  flag: string;
}

/**
 * Supported languages for Gemini Live translate preview
 * This list includes the main languages supported by the service
 */
export const SUPPORTED_LANGUAGES: Language[] = [
  // Major world languages
  { code: 'en', name: 'Inglés', nativeName: 'English', flag: '🇬🇧' },
  { code: 'es', name: 'Español', nativeName: 'Español', flag: '🇪🇸' },
  { code: 'fr', name: 'Francés', nativeName: 'Français', flag: '🇫🇷' },
  { code: 'de', name: 'Alemán', nativeName: 'Deutsch', flag: '🇩🇪' },
  { code: 'it', name: 'Italiano', nativeName: 'Italiano', flag: '🇮🇹' },
  { code: 'pt', name: 'Portugués', nativeName: 'Português', flag: '🇵🇹' },
  { code: 'nl', name: 'Neerlandés', nativeName: 'Nederlands', flag: '🇳🇱' },
  { code: 'pl', name: 'Polaco', nativeName: 'Polski', flag: '🇵🇱' },
  { code: 'ru', name: 'Ruso', nativeName: 'Русский', flag: '🇷🇺' },
  { code: 'uk', name: 'Ucraniano', nativeName: 'Українська', flag: '🇺🇦' },
  
  // Asian languages
  { code: 'zh', name: 'Chino (Mandarín)', nativeName: '中文', flag: '🇨🇳' },
  { code: 'ja', name: 'Japonés', nativeName: '日本語', flag: '🇯🇵' },
  { code: 'ko', name: 'Coreano', nativeName: '한국어', flag: '🇰🇷' },
  { code: 'hi', name: 'Hindi', nativeName: 'हिन्दी', flag: '🇮🇳' },
  { code: 'th', name: 'Tailandés', nativeName: 'ไทย', flag: '🇹🇭' },
  { code: 'vi', name: 'Vietnamita', nativeName: 'Tiếng Việt', flag: '🇻🇳' },
  { code: 'id', name: 'Indonesio', nativeName: 'Bahasa Indonesia', flag: '🇮🇩' },
  
  // Middle Eastern and North African languages
  { code: 'ar', name: 'Árabe', nativeName: 'العربية', flag: '🇸🇦' },
  { code: 'tr', name: 'Turco', nativeName: 'Türkçe', flag: '🇹🇷' },
  { code: 'he', name: 'Hebreo', nativeName: 'עברית', flag: '🇮🇱' },
  
  // Nordic languages
  { code: 'sv', name: 'Sueco', nativeName: 'Svenska', flag: '🇸🇪' },
  { code: 'da', name: 'Danés', nativeName: 'Dansk', flag: '🇩🇰' },
  { code: 'no', name: 'Noruego', nativeName: 'Norsk', flag: '🇳🇴' },
  { code: 'fi', name: 'Finlandés', nativeName: 'Suomi', flag: '🇫🇮' },
  
  // Other European languages
  { code: 'cs', name: 'Checo', nativeName: 'Čeština', flag: '🇨🇿' },
  { code: 'el', name: 'Griego', nativeName: 'Ελληνικά', flag: '🇬🇷' },
  { code: 'hu', name: 'Húngaro', nativeName: 'Magyar', flag: '🇭🇺' },
  { code: 'ro', name: 'Rumano', nativeName: 'Română', flag: '🇷🇴' },
  { code: 'bg', name: 'Búlgaro', nativeName: 'Български', flag: '🇧🇬' },
  { code: 'sk', name: 'Eslovaco', nativeName: 'Slovenčina', flag: '🇸🇰' },
  { code: 'hr', name: 'Croata', nativeName: 'Hrvatski', flag: '🇭🇷' },
];

/**
 * Get a language by its code
 */
export function getLanguageByCode(code: string): Language | undefined {
  return SUPPORTED_LANGUAGES.find(lang => lang.code === code);
}

/**
 * Get display name for a language code
 */
export function getLanguageDisplayName(code: string): string {
  const lang = getLanguageByCode(code);
  return lang ? `${lang.flag} ${lang.name}` : code.toUpperCase();
}

interface LanguageSelectorProps {
  /** Which channel this selector is for */
  channel: ChannelType;
  /** Whether to show source language selector (default: false, only target) */
  showSource?: boolean;
  /** Compact mode for inline display */
  compact?: boolean;
  /** Callback when language changes (for immediate UI updates) */
  onLanguageChange?: (sourceLang: string, targetLang: string) => void;
  /** Additional CSS classes */
  className?: string;
}

/**
 * Language dropdown selector component
 */
function LanguageDropdown({
  value,
  onChange,
  label,
  excludeCode,
  disabled = false,
}: {
  value: string;
  onChange: (code: string) => void;
  label: string;
  excludeCode?: string;
  disabled?: boolean;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  
  const selectedLanguage = getLanguageByCode(value);
  
  // Filter languages based on search and exclusion
  const filteredLanguages = SUPPORTED_LANGUAGES.filter(lang => {
    if (excludeCode && lang.code === excludeCode) return false;
    if (!searchQuery) return true;
    const query = searchQuery.toLowerCase();
    return (
      lang.name.toLowerCase().includes(query) ||
      lang.nativeName.toLowerCase().includes(query) ||
      lang.code.toLowerCase().includes(query)
    );
  });
  
  const handleSelect = useCallback((code: string) => {
    onChange(code);
    setIsOpen(false);
    setSearchQuery('');
  }, [onChange]);
  
  // Close dropdown when clicking outside
  useEffect(() => {
    if (!isOpen) return;
    
    const handleClickOutside = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (!target.closest('.language-dropdown')) {
        setIsOpen(false);
        setSearchQuery('');
      }
    };
    
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isOpen]);
  
  return (
    <div className="language-dropdown relative">
      <label className="block text-xs text-text-secondary mb-1">
        {label}
      </label>
      
      {/* Selected value button */}
      <button
        type="button"
        onClick={() => !disabled && setIsOpen(!isOpen)}
        disabled={disabled}
        className={`
          w-full px-3 py-2 rounded-lg text-left
          bg-surface-hover border border-border
          hover:border-primary/50 transition-colors
          flex items-center justify-between gap-2
          ${disabled ? 'opacity-50 cursor-not-allowed' : 'cursor-pointer'}
          ${isOpen ? 'border-primary ring-1 ring-primary/30' : ''}
        `}
      >
        <span className="flex items-center gap-2 text-sm">
          {selectedLanguage ? (
            <>
              <span className="text-lg">{selectedLanguage.flag}</span>
              <span className="text-text">{selectedLanguage.name}</span>
              <span className="text-text-muted text-xs">
                ({selectedLanguage.nativeName})
              </span>
            </>
          ) : (
            <span className="text-text-muted">Seleccionar idioma</span>
          )}
        </span>
        
        {/* Chevron icon */}
        <svg
          className={`w-4 h-4 text-text-secondary transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      
      {/* Dropdown menu */}
      {isOpen && (
        <div className="absolute z-50 mt-1 w-full bg-surface border border-border rounded-lg shadow-lg overflow-hidden">
          {/* Search input */}
          <div className="p-2 border-b border-border">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Buscar idioma..."
              className="w-full px-3 py-2 text-sm bg-surface-hover rounded-md border border-border
                       focus:border-primary focus:outline-none focus:ring-1 focus:ring-primary/30
                       placeholder:text-text-muted"
              autoFocus
            />
          </div>
          
          {/* Language list */}
          <div className="max-h-60 overflow-y-auto">
            {filteredLanguages.length === 0 ? (
              <div className="px-3 py-4 text-center text-text-muted text-sm">
                No se encontraron idiomas
              </div>
            ) : (
              filteredLanguages.map(lang => (
                <button
                  key={lang.code}
                  type="button"
                  onClick={() => handleSelect(lang.code)}
                  className={`
                    w-full px-3 py-2 text-left text-sm
                    flex items-center gap-2
                    hover:bg-surface-hover transition-colors
                    ${lang.code === value ? 'bg-primary/10 text-primary' : 'text-text'}
                  `}
                >
                  <span className="text-lg">{lang.flag}</span>
                  <span className="flex-1">{lang.name}</span>
                  <span className="text-text-muted text-xs">{lang.nativeName}</span>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/**
 * Main Language Selector component
 * 
 * Provides UI for selecting source and target languages for translation channels.
 * Persists selections to configuration and triggers updates for next Gemini session.
 */
export function LanguageSelector({
  channel,
  showSource = false,
  compact = false,
  onLanguageChange,
  className = '',
}: LanguageSelectorProps) {
  const { config, updateConfig, saveConfig, loading } = useConfig();
  const [isSaving, setIsSaving] = useState(false);
  
  // Get current languages for this channel
  const sourceLang = channel === 'system' 
    ? config.languages.systemSourceLang 
    : config.languages.userSourceLang;
  
  const targetLang = channel === 'system'
    ? config.languages.systemTargetLang
    : config.languages.userTargetLang;
  
  /**
   * Handle source language change
   * Updates config and persists between sessions
   * @see Requirement 15.4 - Persist selection between sessions
   * @see Requirement 15.5 - Update Gemini config for next session
   */
  const handleSourceChange = useCallback(async (newSourceLang: string) => {
    // Don't allow same source and target
    if (newSourceLang === targetLang) {
      return;
    }
    
    // Update config based on channel
    const languageUpdates = channel === 'system'
      ? { systemSourceLang: newSourceLang }
      : { userSourceLang: newSourceLang };
    
    updateConfig({
      languages: {
        ...config.languages,
        ...languageUpdates,
      },
    });
    
    // Persist to storage for next session
    setIsSaving(true);
    try {
      // The saveConfig will persist to SQLite
      await saveConfig();
      
      // Notify parent component for immediate UI updates
      onLanguageChange?.(newSourceLang, targetLang);
    } finally {
      setIsSaving(false);
    }
  }, [channel, config.languages, targetLang, updateConfig, saveConfig, onLanguageChange]);
  
  /**
   * Handle target language change
   * Updates config and persists between sessions
   * @see Requirement 15.1 - Select target language for system channel
   * @see Requirement 15.2 - Select target language for user channel
   * @see Requirement 15.4 - Persist selection between sessions
   * @see Requirement 15.5 - Update Gemini config for next session
   */
  const handleTargetChange = useCallback(async (newTargetLang: string) => {
    // Don't allow same source and target
    if (newTargetLang === sourceLang) {
      return;
    }
    
    // Update config based on channel
    const languageUpdates = channel === 'system'
      ? { systemTargetLang: newTargetLang }
      : { userTargetLang: newTargetLang };
    
    updateConfig({
      languages: {
        ...config.languages,
        ...languageUpdates,
      },
    });
    
    // Persist to storage for next session
    setIsSaving(true);
    try {
      // The saveConfig will persist to SQLite
      await saveConfig();
      
      // Notify parent component for immediate UI updates
      onLanguageChange?.(sourceLang, newTargetLang);
    } finally {
      setIsSaving(false);
    }
  }, [channel, config.languages, sourceLang, updateConfig, saveConfig, onLanguageChange]);
  
  // Channel labels
  const channelLabel = channel === 'system' 
    ? 'Canal Sistema (Reunión → Usuario)'
    : 'Canal Usuario (Usuario → Reunión)';
  
  const channelDescription = channel === 'system'
    ? 'Traduce el audio de la reunión a tu idioma'
    : 'Traduce tu voz al idioma de la reunión';
  
  // Compact mode - just show dropdowns inline
  if (compact) {
    return (
      <div className={`flex items-center gap-2 ${className}`}>
        {showSource && (
          <>
            <div className="min-w-[120px]">
              <LanguageDropdown
                value={sourceLang}
                onChange={handleSourceChange}
                label="Origen"
                excludeCode={targetLang}
                disabled={loading || isSaving}
              />
            </div>
            <span className="text-text-secondary">→</span>
          </>
        )}
        <div className="min-w-[120px]">
          <LanguageDropdown
            value={targetLang}
            onChange={handleTargetChange}
            label="Destino"
            excludeCode={sourceLang}
            disabled={loading || isSaving}
          />
        </div>
        
        {/* Saving indicator */}
        {isSaving && (
          <svg className="animate-spin h-4 w-4 text-primary" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
        )}
      </div>
    );
  }
  
  // Full mode - card with header and descriptions
  return (
    <div className={`bg-surface rounded-xl p-4 border border-border ${className}`}>
      {/* Header */}
      <div className="mb-3">
        <h3 className="text-sm font-medium text-text flex items-center gap-2">
          <svg className="w-4 h-4 text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
              d="M3 5h12M9 3v2m1.048 9.5A18.022 18.022 0 016.412 9m6.088 9h7M11 21l5-10 5 10M12.751 5C11.783 10.77 8.07 15.61 3 18.129" />
          </svg>
          {channelLabel}
        </h3>
        <p className="text-xs text-text-muted mt-0.5">{channelDescription}</p>
      </div>
      
      {/* Language selectors */}
      <div className={`grid gap-3 ${showSource ? 'grid-cols-2' : 'grid-cols-1'}`}>
        {showSource && (
          <LanguageDropdown
            value={sourceLang}
            onChange={handleSourceChange}
            label="Idioma de origen"
            excludeCode={targetLang}
            disabled={loading || isSaving}
          />
        )}
        
        <LanguageDropdown
          value={targetLang}
          onChange={handleTargetChange}
          label="Idioma de destino"
          excludeCode={sourceLang}
          disabled={loading || isSaving}
        />
      </div>
      
      {/* Visual language pair indicator */}
      <div className="mt-3 flex items-center justify-center gap-2 py-2 px-3 bg-surface-hover rounded-lg">
        <span className="flex items-center gap-1">
          <span className="text-lg">{getLanguageByCode(sourceLang)?.flag || '🌐'}</span>
          <span className="text-sm text-text">{getLanguageByCode(sourceLang)?.code.toUpperCase() || sourceLang.toUpperCase()}</span>
        </span>
        
        <svg className="w-5 h-5 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
        </svg>
        
        <span className="flex items-center gap-1">
          <span className="text-lg">{getLanguageByCode(targetLang)?.flag || '🌐'}</span>
          <span className="text-sm text-primary font-medium">{getLanguageByCode(targetLang)?.code.toUpperCase() || targetLang.toUpperCase()}</span>
        </span>
        
        {/* Saving indicator */}
        {isSaving && (
          <svg className="animate-spin h-4 w-4 text-primary ml-2" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
        )}
      </div>
    </div>
  );
}

/**
 * Dual Language Selector - shows both channels side by side
 * Convenient for settings pages where both channels need configuration
 */
export function DualLanguageSelector({
  showSource = true,
  className = '',
}: {
  showSource?: boolean;
  className?: string;
}) {
  return (
    <div className={`grid grid-cols-1 md:grid-cols-2 gap-4 ${className}`}>
      <LanguageSelector
        channel="system"
        showSource={showSource}
      />
      <LanguageSelector
        channel="user"
        showSource={showSource}
      />
    </div>
  );
}

/**
 * Quick Language Swap button
 * Swaps source and target languages for a channel
 */
export function LanguageSwapButton({
  channel,
  className = '',
}: {
  channel: ChannelType;
  className?: string;
}) {
  const { config, updateConfig, saveConfig } = useConfig();
  const [isSwapping, setIsSwapping] = useState(false);
  
  const handleSwap = useCallback(async () => {
    setIsSwapping(true);
    
    try {
      const languageUpdates = channel === 'system'
        ? {
            systemSourceLang: config.languages.systemTargetLang,
            systemTargetLang: config.languages.systemSourceLang,
          }
        : {
            userSourceLang: config.languages.userTargetLang,
            userTargetLang: config.languages.userSourceLang,
          };
      
      updateConfig({
        languages: {
          ...config.languages,
          ...languageUpdates,
        },
      });
      
      await saveConfig();
    } finally {
      setIsSwapping(false);
    }
  }, [channel, config.languages, updateConfig, saveConfig]);
  
  return (
    <button
      type="button"
      onClick={handleSwap}
      disabled={isSwapping}
      className={`
        p-2 rounded-lg border border-border
        hover:bg-surface-hover hover:border-primary/50
        transition-all duration-200
        disabled:opacity-50 disabled:cursor-not-allowed
        ${className}
      `}
      title="Intercambiar idiomas"
    >
      {isSwapping ? (
        <svg className="animate-spin h-5 w-5 text-primary" viewBox="0 0 24 24">
          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
        </svg>
      ) : (
        <svg className="w-5 h-5 text-text-secondary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
            d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
        </svg>
      )}
    </button>
  );
}

export default LanguageSelector;
