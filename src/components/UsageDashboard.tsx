/**
 * Usage Dashboard Component
 * 
 * Displays translation usage statistics including:
 * - Current month usage vs plan limit with visual progress bar
 * - Daily usage chart for the last 30 days
 * 
 * @module components/UsageDashboard
 * @see Requirement 11.4 - Show dashboard with minutes used vs limit and daily usage graph
 */

import { useMemo } from 'react';
import { useUsage } from '../hooks/useUsage';
import type { DailyUsage } from '../ipc/types';

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/**
 * Formats a number to a localized string with thousand separators
 */
function formatNumber(num: number): string {
  return num.toLocaleString('es-ES');
}

/**
 * Formats a date string to a short format (e.g., "15 Ene")
 */
function formatDateShort(dateStr: string): string {
  const date = new Date(dateStr);
  const months = ['Ene', 'Feb', 'Mar', 'Abr', 'May', 'Jun', 'Jul', 'Ago', 'Sep', 'Oct', 'Nov', 'Dic'];
  return `${date.getDate()} ${months[date.getMonth()]}`;
}

/**
 * Gets the color class based on usage percentage
 */
function getUsageColor(percentage: number): { bg: string; text: string; bar: string } {
  if (percentage >= 100) {
    return { bg: 'bg-error/10', text: 'text-error', bar: 'bg-error' };
  }
  if (percentage >= 80) {
    return { bg: 'bg-warning/10', text: 'text-warning', bar: 'bg-warning' };
  }
  return { bg: 'bg-primary/10', text: 'text-primary', bar: 'bg-primary' };
}

/**
 * Gets the plan display name in Spanish
 */
function getPlanDisplayName(plan: string): string {
  const names: Record<string, string> = {
    'byok_free': 'BYOK Gratuito',
    'starter': 'Starter',
    'pro': 'Pro',
  };
  return names[plan] || plan;
}

// ============================================================================
// SUB-COMPONENTS
// ============================================================================

/** Progress bar for usage visualization */
function UsageProgressBar({ 
  used, 
  limit, 
  percentage 
}: { 
  used: number; 
  limit: number; 
  percentage: number;
}) {
  const colors = getUsageColor(percentage);
  const isUnlimited = limit === 0;
  
  return (
    <div className="space-y-2">
      <div className="flex justify-between items-baseline">
        <span className="text-2xl font-bold text-text">
          {formatNumber(used)}
          <span className="text-sm font-normal text-text-secondary ml-1">min</span>
        </span>
        {isUnlimited ? (
          <span className="text-sm text-text-secondary">Sin límite</span>
        ) : (
          <span className="text-sm text-text-secondary">
            de {formatNumber(limit)} min
          </span>
        )}
      </div>
      
      <div className="w-full h-3 bg-surface-hover rounded-full overflow-hidden">
        <div 
          className={`h-full ${colors.bar} transition-all duration-500 ease-out rounded-full`}
          style={{ width: `${isUnlimited ? 0 : Math.min(percentage, 100)}%` }}
        />
      </div>
      
      {!isUnlimited && (
        <div className="flex justify-between text-xs text-text-secondary">
          <span className={colors.text}>{percentage.toFixed(1)}% usado</span>
          <span>{formatNumber(Math.max(0, limit - used))} min restantes</span>
        </div>
      )}
    </div>
  );
}

/** Single bar in the daily usage chart */
function DailyBar({ 
  usage, 
  maxMinutes, 
  isToday 
}: { 
  usage: DailyUsage; 
  maxMinutes: number; 
  isToday: boolean;
}) {
  const heightPercentage = maxMinutes > 0 
    ? Math.max(2, (usage.totalMinutes / maxMinutes) * 100) 
    : 2;
  
  const systemHeight = maxMinutes > 0 && usage.totalMinutes > 0
    ? (usage.systemMinutes / usage.totalMinutes) * 100
    : 50;
  
  return (
    <div className="flex flex-col items-center flex-1 min-w-0 group">
      <div className="relative w-full h-32 flex items-end justify-center mb-1">
        <div 
          className={`w-2/3 max-w-[12px] rounded-t transition-all duration-200 group-hover:opacity-80
            ${isToday ? 'bg-secondary' : 'bg-primary'}`}
          style={{ height: `${heightPercentage}%` }}
        >
          {/* System vs User split indicator */}
          {usage.totalMinutes > 0 && (
            <div 
              className="absolute bottom-0 left-0 right-0 bg-primary/60 rounded-t"
              style={{ height: `${systemHeight}%` }}
            />
          )}
        </div>
        
        {/* Tooltip on hover */}
        <div className="absolute bottom-full mb-2 hidden group-hover:block z-10">
          <div className="bg-surface border border-border rounded-lg shadow-lg p-2 text-xs whitespace-nowrap">
            <div className="font-medium text-text mb-1">
              {formatDateShort(usage.date)}
            </div>
            <div className="text-text-secondary">
              Total: {usage.totalMinutes} min
            </div>
            <div className="text-primary text-xs">
              Sistema: {usage.systemMinutes} min
            </div>
            <div className="text-secondary text-xs">
              Usuario: {usage.userMinutes} min
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/** Chart showing daily usage for the last 30 days */
function DailyUsageChart({ history }: { history: DailyUsage[] }) {
  // Calculate max minutes for scaling
  const maxMinutes = useMemo(() => {
    return Math.max(...history.map(d => d.totalMinutes), 1);
  }, [history]);
  
  // Get today's date for highlighting
  const today = new Date().toISOString().split('T')[0];
  
  // Show labels for every 5th day or so
  const labelInterval = Math.ceil(history.length / 6);
  
  if (history.length === 0) {
    return (
      <div className="h-40 flex items-center justify-center text-text-secondary text-sm">
        No hay datos de uso disponibles
      </div>
    );
  }
  
  return (
    <div className="space-y-2">
      {/* Chart bars */}
      <div className="flex items-end gap-0.5 h-36">
        {history.map((usage) => (
          <DailyBar 
            key={usage.date}
            usage={usage}
            maxMinutes={maxMinutes}
            isToday={usage.date === today}
          />
        ))}
      </div>
      
      {/* X-axis labels */}
      <div className="flex justify-between text-xs text-text-secondary px-1">
        {history.filter((_, i) => i % labelInterval === 0 || i === history.length - 1).map(usage => (
          <span key={usage.date} className="text-center">
            {formatDateShort(usage.date)}
          </span>
        ))}
      </div>
      
      {/* Legend */}
      <div className="flex items-center justify-center gap-4 text-xs text-text-secondary pt-2">
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 rounded bg-primary" />
          <span>Canal Sistema</span>
        </div>
        <div className="flex items-center gap-1">
          <div className="w-3 h-3 rounded bg-secondary" />
          <span>Hoy</span>
        </div>
      </div>
    </div>
  );
}

/** Stats card showing key metrics */
function StatsCard({ 
  label, 
  value, 
  sublabel,
  colorClass = 'text-text'
}: { 
  label: string; 
  value: string | number;
  sublabel?: string;
  colorClass?: string;
}) {
  return (
    <div className="bg-surface-hover rounded-lg p-3 text-center">
      <div className="text-xs text-text-secondary mb-1">{label}</div>
      <div className={`text-lg font-semibold ${colorClass}`}>{value}</div>
      {sublabel && (
        <div className="text-xs text-text-secondary mt-0.5">{sublabel}</div>
      )}
    </div>
  );
}

// ============================================================================
// MAIN COMPONENT
// ============================================================================

export interface UsageDashboardProps {
  /** Optional class name for custom styling */
  className?: string;
  /** Whether to show the chart (default: true) */
  showChart?: boolean;
  /** Whether to show compact mode (header only, no chart) */
  compact?: boolean;
}

/**
 * Usage Dashboard Component
 * 
 * Displays current usage statistics and daily usage history chart.
 * Integrates with useUsage hook for data fetching.
 * 
 * @example
 * // Full dashboard
 * <UsageDashboard />
 * 
 * // Compact mode (no chart)
 * <UsageDashboard compact />
 * 
 * // With custom styling
 * <UsageDashboard className="mt-4" />
 */
export function UsageDashboard({ 
  className = '', 
  showChart = true,
  compact = false 
}: UsageDashboardProps) {
  const { stats, history, loading, error, limitReached, refresh } = useUsage();
  
  // Calculate additional stats from history
  const weeklyTotal = useMemo(() => {
    const last7Days = history.slice(-7);
    return last7Days.reduce((sum, day) => sum + day.totalMinutes, 0);
  }, [history]);
  
  const averageDaily = useMemo(() => {
    if (history.length === 0) return 0;
    const total = history.reduce((sum, day) => sum + day.totalMinutes, 0);
    return Math.round(total / history.length);
  }, [history]);
  
  // Loading state
  if (loading && !stats) {
    return (
      <div className={`bg-surface rounded-xl p-6 border border-border ${className}`}>
        <div className="animate-pulse space-y-4">
          <div className="h-6 bg-surface-hover rounded w-1/3" />
          <div className="h-3 bg-surface-hover rounded w-full" />
          <div className="h-32 bg-surface-hover rounded" />
        </div>
      </div>
    );
  }
  
  // Error state
  if (error) {
    return (
      <div className={`bg-surface rounded-xl p-6 border border-border ${className}`}>
        <div className="text-center py-4">
          <div className="text-error mb-2">Error al cargar uso</div>
          <p className="text-text-secondary text-sm mb-4">{error}</p>
          <button 
            onClick={refresh}
            className="px-4 py-2 bg-primary text-white rounded-lg hover:opacity-90 transition"
          >
            Reintentar
          </button>
        </div>
      </div>
    );
  }
  
  // No data state
  if (!stats) {
    return (
      <div className={`bg-surface rounded-xl p-6 border border-border ${className}`}>
        <div className="text-center py-4 text-text-secondary">
          No hay datos de uso disponibles
        </div>
      </div>
    );
  }
  
  const isUnlimited = stats.currentMonth.limit === 0;
  const colors = getUsageColor(stats.currentMonth.percentage);
  
  // Compact mode - just show progress bar
  if (compact) {
    return (
      <div className={`bg-surface rounded-xl p-4 border border-border ${className}`}>
        <div className="flex items-center justify-between mb-2">
          <h3 className="text-sm font-medium text-text">Uso del Mes</h3>
          <span className="text-xs text-text-secondary">
            Plan {getPlanDisplayName(stats.plan)}
          </span>
        </div>
        <UsageProgressBar 
          used={stats.currentMonth.used}
          limit={stats.currentMonth.limit}
          percentage={stats.currentMonth.percentage}
        />
      </div>
    );
  }
  
  return (
    <div className={`bg-surface rounded-xl p-6 border border-border ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div>
          <h2 className="text-lg font-semibold text-text">Dashboard de Uso</h2>
          <p className="text-sm text-text-secondary">
            Plan {getPlanDisplayName(stats.plan)}
          </p>
        </div>
        
        {limitReached && (
          <span className={`px-3 py-1 rounded-full text-xs font-medium ${colors.bg} ${colors.text}`}>
            Límite alcanzado
          </span>
        )}
        
        {!limitReached && !isUnlimited && stats.currentMonth.percentage >= 80 && (
          <span className={`px-3 py-1 rounded-full text-xs font-medium ${colors.bg} ${colors.text}`}>
            {stats.currentMonth.percentage.toFixed(0)}% usado
          </span>
        )}
        
        {isUnlimited && (
          <span className="px-3 py-1 rounded-full text-xs font-medium bg-success/10 text-success">
            Sin límites
          </span>
        )}
      </div>
      
      {/* Main usage progress */}
      <div className={`p-4 rounded-lg mb-6 ${colors.bg}`}>
        <UsageProgressBar 
          used={stats.currentMonth.used}
          limit={stats.currentMonth.limit}
          percentage={stats.currentMonth.percentage}
        />
      </div>
      
      {/* Quick stats */}
      <div className="grid grid-cols-3 gap-3 mb-6">
        <StatsCard 
          label="Esta Semana" 
          value={`${formatNumber(weeklyTotal)} min`}
        />
        <StatsCard 
          label="Promedio Diario" 
          value={`${averageDaily} min`}
        />
        <StatsCard 
          label="Renovación" 
          value={formatDateShort(stats.renewalDate)}
          sublabel={new Date(stats.renewalDate).toLocaleDateString('es-ES')}
        />
      </div>
      
      {/* Daily usage chart */}
      {showChart && (
        <div>
          <h3 className="text-sm font-medium text-text mb-3">
            Uso Diario (Últimos 30 días)
          </h3>
          <DailyUsageChart history={history} />
        </div>
      )}
      
      {/* Limit reached warning */}
      {limitReached && (
        <div className="mt-4 p-4 bg-error/10 border border-error/20 rounded-lg">
          <div className="flex items-start gap-3">
            <svg className="w-5 h-5 text-error flex-shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
            <div>
              <p className="text-error font-medium">Límite de minutos alcanzado</p>
              <p className="text-text-secondary text-sm mt-1">
                Has usado todos los minutos de tu plan este mes. 
                Actualiza tu plan para continuar traduciendo o espera al próximo ciclo de facturación.
              </p>
            </div>
          </div>
        </div>
      )}
      
      {/* Refresh button */}
      <div className="mt-4 flex justify-end">
        <button 
          onClick={refresh}
          disabled={loading}
          className="text-sm text-text-secondary hover:text-text transition flex items-center gap-1"
        >
          <svg 
            className={`w-4 h-4 ${loading ? 'animate-spin' : ''}`} 
            fill="none" 
            viewBox="0 0 24 24" 
            stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
          {loading ? 'Actualizando...' : 'Actualizar'}
        </button>
      </div>
    </div>
  );
}

export default UsageDashboard;
