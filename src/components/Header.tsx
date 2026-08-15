/**
 * Header Component
 * 
 * Application header with logo, status indicators, and settings button.
 * Adapted from web project to use Tauri IPC for desktop functionality.
 * 
 * @module components/Header
 * @see Requirement 12.1 - Reuse components from web project
 * @see Requirement 12.2 - Replace fetch/WebSocket with Tauri IPC
 * @see Requirement 12.7 - Dark theme with indigo/cyan accents
 */

import { useAudioEngine } from '../hooks/useAudioEngine';
import { useAuth } from '../hooks/useAuth';
import { useUsage } from '../hooks/useUsage';

interface HeaderProps {
  onSettingsClick: () => void;
}

/** Connection status indicator */
function StatusBadge({ 
  isTranslating, 
  hasError 
}: { 
  isTranslating: boolean; 
  hasError: boolean;
}) {
  if (hasError) {
    return (
      <span className="flex items-center gap-1.5 px-2 py-1 bg-error/10 text-error text-xs rounded-full">
        <span className="w-2 h-2 rounded-full bg-error" />
        Error
      </span>
    );
  }
  
  if (isTranslating) {
    return (
      <span className="flex items-center gap-1.5 px-2 py-1 bg-success/10 text-success text-xs rounded-full">
        <span className="w-2 h-2 rounded-full bg-success animate-pulse" />
        Traduciendo
      </span>
    );
  }
  
  return (
    <span className="flex items-center gap-1.5 px-2 py-1 bg-surface-hover text-text-secondary text-xs rounded-full">
      <span className="w-2 h-2 rounded-full bg-gray-500" />
      Inactivo
    </span>
  );
}

/** Usage indicator component */
function UsageIndicator({ 
  used, 
  limit, 
  percentage 
}: { 
  used: number; 
  limit: number; 
  percentage: number;
}) {
  // Don't show for BYOK users (limit = 0)
  if (limit === 0) {
    return (
      <span className="text-xs text-text-secondary">
        BYOK • Sin límites
      </span>
    );
  }
  
  const getColor = () => {
    if (percentage >= 100) return 'text-error';
    if (percentage >= 80) return 'text-warning';
    return 'text-text-secondary';
  };
  
  return (
    <span className={`text-xs ${getColor()}`}>
      {used}/{limit} min ({percentage.toFixed(0)}%)
    </span>
  );
}

export function Header({ onSettingsClick }: HeaderProps) {
  const { isTranslating, systemChannelState, userChannelState, metrics } = useAudioEngine();
  const { user, hasByokKey, isAuthenticated } = useAuth();
  const { stats } = useUsage();
  
  // Check if there's an error in either channel
  const hasError = systemChannelState.type === 'error' || userChannelState.type === 'error';
  
  return (
    <header className="bg-surface border-b border-border px-6 py-4">
      <div className="flex items-center justify-between">
        {/* Left side - Logo and title */}
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 bg-gradient-to-br from-primary to-secondary rounded-lg flex items-center justify-center shadow-lg">
            <svg className="w-5 h-5 text-white" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 5h12M9 3v2m1.048 9.5A18.022 18.022 0 016.412 9m6.088 9h7M11 21l5-10 5 10M12.751 5C11.783 10.77 8.07 15.61 3 18.129" />
            </svg>
          </div>
          <div>
            <h1 className="text-lg font-semibold text-text">Traductor Desktop</h1>
            <div className="flex items-center gap-2">
              <span className="text-xs text-text-secondary">v1.0.0</span>
              {isTranslating && metrics && (
                <span className="text-xs text-primary">
                  • {metrics.latencyMs}ms
                </span>
              )}
            </div>
          </div>
        </div>
        
        {/* Center - Status */}
        <div className="flex items-center gap-3">
          <StatusBadge isTranslating={isTranslating} hasError={hasError} />
          
          {stats && (
            <UsageIndicator 
              used={stats.currentMonth.used} 
              limit={stats.currentMonth.limit} 
              percentage={stats.currentMonth.percentage} 
            />
          )}
        </div>
        
        {/* Right side - User and Settings */}
        <div className="flex items-center gap-3">
          {/* User info */}
          {isAuthenticated && user ? (
            <div className="flex items-center gap-2">
              <div className="w-8 h-8 bg-primary/20 rounded-full flex items-center justify-center">
                <span className="text-primary text-sm font-medium">
                  {user.name?.charAt(0).toUpperCase() || user.email?.charAt(0).toUpperCase() || 'U'}
                </span>
              </div>
              <span className="text-sm text-text-secondary hidden sm:inline">
                {user.name || user.email}
              </span>
            </div>
          ) : hasByokKey ? (
            <span className="text-sm text-text-secondary">Modo BYOK</span>
          ) : (
            <span className="text-sm text-text-secondary">No conectado</span>
          )}
          
          {/* Settings button */}
          <button
            onClick={onSettingsClick}
            className="p-2 text-text-secondary hover:text-text rounded-lg hover:bg-surface-hover transition"
            aria-label="Configuración"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
          </button>
        </div>
      </div>
    </header>
  );
}
