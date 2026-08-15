/**
 * Subscription Page Component
 * 
 * Page for managing subscriptions, viewing plan comparison,
 * upgrading/downgrading plans, and accessing invoice history.
 * 
 * @module components/SubscriptionPage
 * @see Requirement 10.2 - Show comparative table of plans
 * @see Requirement 10.5 - Show last 24 invoices as downloadable PDF
 * @see Requirement 8.8 - Show indication of no limits in BYOK mode
 */

import { useState, useEffect, useCallback } from 'react';
import { useAuth } from '../hooks/useAuth';
import { useUsage } from '../hooks/useUsage';
import { getUpgradeOptions } from '../ipc/commands';
import type { 
  SubscriptionPlan, 
  PlanDetails, 
  Invoice, 
  UpgradeOption 
} from '../ipc/types';
import { PLAN_DETAILS } from '../ipc/types';

interface SubscriptionPageProps {
  /** Whether the page is displayed (for mounting/unmounting) */
  isVisible?: boolean;
  /** Callback when user navigates back */
  onBack?: () => void;
}

/** Plan comparison card component */
function PlanCard({ 
  plan, 
  details, 
  isCurrentPlan,
  onSelect,
  isUpgrade,
  isDowngrade,
  disabled,
}: { 
  plan: SubscriptionPlan;
  details: PlanDetails;
  isCurrentPlan: boolean;
  isByokMode?: boolean;
  onSelect?: () => void;
  isUpgrade?: boolean;
  isDowngrade?: boolean;
  disabled?: boolean;
}) {
  // Determine if this is the recommended plan
  const isRecommended = plan === 'starter' && !isCurrentPlan;
  
  return (
    <div 
      className={`relative flex flex-col p-6 rounded-xl border transition-all
        ${isCurrentPlan 
          ? 'border-primary bg-primary/5' 
          : 'border-border bg-surface hover:border-primary/50'
        }
        ${disabled ? 'opacity-50' : ''}
      `}
    >
      {/* Recommended badge */}
      {isRecommended && (
        <div className="absolute -top-3 left-1/2 -translate-x-1/2">
          <span className="px-3 py-1 text-xs font-medium bg-primary text-white rounded-full">
            Recomendado
          </span>
        </div>
      )}
      
      {/* Current plan badge */}
      {isCurrentPlan && (
        <div className="absolute -top-3 left-1/2 -translate-x-1/2">
          <span className="px-3 py-1 text-xs font-medium bg-success text-white rounded-full">
            Plan actual
          </span>
        </div>
      )}
      
      {/* Plan name */}
      <h3 className="text-lg font-semibold text-text mb-2">{details.name}</h3>
      
      {/* Price */}
      <div className="mb-4">
        <span className="text-3xl font-bold text-text">
          ${details.price.toFixed(2)}
        </span>
        <span className="text-text-secondary">/mes</span>
      </div>
      
      {/* Minutes limit */}
      <div className="mb-4 pb-4 border-b border-border">
        {details.minutesLimit === 0 ? (
          <div className="flex items-center gap-2">
            <svg className="w-5 h-5 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z" />
            </svg>
            <span className="text-accent font-medium">Uso ilimitado</span>
          </div>
        ) : (
          <span className="text-text-secondary">
            {details.minutesLimit.toLocaleString()} minutos/mes
          </span>
        )}
      </div>
      
      {/* Features */}
      <ul className="flex-1 space-y-2 mb-6">
        {details.features.map((feature, idx) => (
          <li key={idx} className="flex items-start gap-2 text-sm text-text-secondary">
            <svg className="w-4 h-4 text-success mt-0.5 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
            </svg>
            <span>{feature}</span>
          </li>
        ))}
      </ul>
      
      {/* Action button */}
      {!isCurrentPlan && onSelect && (
        <button
          onClick={onSelect}
          disabled={disabled}
          className={`w-full py-2 px-4 rounded-lg font-medium transition
            ${isUpgrade 
              ? 'bg-primary text-white hover:opacity-90' 
              : 'bg-surface-hover text-text hover:bg-border'
            }
            disabled:opacity-50 disabled:cursor-not-allowed
          `}
        >
          {isUpgrade ? 'Actualizar' : isDowngrade ? 'Cambiar plan' : 'Seleccionar'}
        </button>
      )}
      
      {isCurrentPlan && (
        <div className="w-full py-2 px-4 text-center text-text-secondary text-sm">
          Tu plan actual
        </div>
      )}
    </div>
  );
}

/** Invoice row component */
function InvoiceRow({ invoice, onDownload }: { invoice: Invoice; onDownload: () => void }) {
  const formattedDate = new Date(invoice.date).toLocaleDateString('es-ES', {
    year: 'numeric',
    month: 'long',
    day: 'numeric'
  });
  
  const formattedAmount = (invoice.amountCents / 100).toFixed(2);
  
  const statusColors = {
    paid: 'text-success bg-success/10',
    pending: 'text-warning bg-warning/10',
    failed: 'text-error bg-error/10',
  };
  
  const statusLabels = {
    paid: 'Pagada',
    pending: 'Pendiente',
    failed: 'Fallida',
  };
  
  return (
    <div className="flex items-center justify-between py-3 px-4 hover:bg-surface-hover rounded-lg transition">
      <div className="flex items-center gap-4">
        <div className="p-2 bg-surface-hover rounded-lg">
          <svg className="w-5 h-5 text-text-secondary" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
        </div>
        <div>
          <p className="text-text font-medium">Factura #{invoice.id.slice(-8)}</p>
          <p className="text-sm text-text-secondary">{formattedDate}</p>
        </div>
      </div>
      
      <div className="flex items-center gap-4">
        <span className={`px-2 py-1 text-xs font-medium rounded-full ${statusColors[invoice.status]}`}>
          {statusLabels[invoice.status]}
        </span>
        <span className="text-text font-medium">${formattedAmount}</span>
        
        {invoice.pdfUrl && (
          <button
            onClick={onDownload}
            className="p-2 text-text-secondary hover:text-primary hover:bg-primary/10 rounded-lg transition"
            title="Descargar PDF"
          >
            <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}

/** BYOK unlimited indicator component */
function ByokUnlimitedBanner() {
  return (
    <div className="bg-gradient-to-r from-accent/20 to-primary/20 border border-accent/30 rounded-xl p-6 mb-6">
      <div className="flex items-start gap-4">
        <div className="p-3 bg-accent/20 rounded-full">
          <svg className="w-6 h-6 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z" />
          </svg>
        </div>
        <div className="flex-1">
          <h3 className="text-lg font-semibold text-text mb-1">Modo BYOK Activo</h3>
          <p className="text-text-secondary mb-2">
            Estás usando tu propia API key de Gemini. No tienes límites de minutos mensuales.
          </p>
          <p className="text-sm text-text-secondary">
            Los costos de uso se facturan directamente a través de tu cuenta de Google Cloud.
          </p>
        </div>
      </div>
    </div>
  );
}

/** Usage summary component */
function UsageSummary({ 
  used, 
  limit, 
  percentage, 
  renewalDate 
}: { 
  used: number; 
  limit: number; 
  percentage: number;
  renewalDate: string;
}) {
  const formattedRenewal = new Date(renewalDate).toLocaleDateString('es-ES', {
    year: 'numeric',
    month: 'long',
    day: 'numeric'
  });
  
  const progressColor = percentage >= 100 
    ? 'bg-error' 
    : percentage >= 80 
      ? 'bg-warning' 
      : 'bg-primary';
  
  return (
    <div className="bg-surface border border-border rounded-xl p-6 mb-6">
      <h3 className="text-lg font-semibold text-text mb-4">Uso del mes actual</h3>
      
      <div className="flex items-end justify-between mb-2">
        <span className="text-2xl font-bold text-text">
          {used.toLocaleString()} <span className="text-lg font-normal text-text-secondary">min</span>
        </span>
        <span className="text-text-secondary">
          de {limit.toLocaleString()} min
        </span>
      </div>
      
      {/* Progress bar */}
      <div className="h-3 bg-surface-hover rounded-full overflow-hidden mb-4">
        <div 
          className={`h-full ${progressColor} transition-all duration-300`}
          style={{ width: `${Math.min(percentage, 100)}%` }}
        />
      </div>
      
      <div className="flex items-center justify-between text-sm text-text-secondary">
        <span>{percentage.toFixed(1)}% utilizado</span>
        <span>Se renueva el {formattedRenewal}</span>
      </div>
      
      {percentage >= 80 && percentage < 100 && (
        <div className="mt-4 p-3 bg-warning/10 border border-warning/30 rounded-lg">
          <p className="text-sm text-warning">
            ⚠️ Estás cerca de alcanzar tu límite mensual. Considera actualizar tu plan.
          </p>
        </div>
      )}
      
      {percentage >= 100 && (
        <div className="mt-4 p-3 bg-error/10 border border-error/30 rounded-lg">
          <p className="text-sm text-error">
            ❌ Has alcanzado tu límite mensual. Actualiza tu plan para continuar usando la traducción.
          </p>
        </div>
      )}
    </div>
  );
}

export function SubscriptionPage({ isVisible = true, onBack }: SubscriptionPageProps) {
  const { user, isByokMode, loading: authLoading } = useAuth();
  const { stats, loading: usageLoading } = useUsage();
  
  const [, setUpgradeOptions] = useState<UpgradeOption[]>([]);
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [loadingInvoices, setLoadingInvoices] = useState(true);
  const [isProcessing, setIsProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // Determine current plan
  const currentPlan: SubscriptionPlan = isByokMode 
    ? 'byok_free' 
    : (user?.plan as SubscriptionPlan) || 'byok_free';
  
  // Load upgrade options and invoices
  useEffect(() => {
    async function loadData() {
      try {
        // Load upgrade options
        const options = await getUpgradeOptions();
        setUpgradeOptions(options);
        
        // TODO: Load invoices from billing service when implemented
        // For now, use mock data for display purposes
        setInvoices(generateMockInvoices());
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Error al cargar datos');
      } finally {
        setLoadingInvoices(false);
      }
    }
    
    if (isVisible) {
      loadData();
    }
  }, [isVisible]);
  
  // Generate mock invoices for display (to be replaced with real billing API)
  const generateMockInvoices = useCallback((): Invoice[] => {
    if (currentPlan === 'byok_free') return [];
    
    const planPrice = PLAN_DETAILS[currentPlan].price;
    const mockInvoices: Invoice[] = [];
    
    // Generate up to 24 mock invoices for demonstration
    for (let i = 0; i < 12; i++) {
      const date = new Date();
      date.setMonth(date.getMonth() - i);
      
      mockInvoices.push({
        id: `inv_${Date.now()}_${i}`,
        date: date.toISOString(),
        amountCents: Math.round(planPrice * 100),
        status: i === 0 ? 'pending' : 'paid',
        pdfUrl: i === 0 ? undefined : `https://billing.example.com/invoices/inv_${i}.pdf`,
        downloaded: false,
      });
    }
    
    return mockInvoices;
  }, [currentPlan]);
  
  // Handle plan selection (upgrade/downgrade)
  const handlePlanSelect = useCallback(async (plan: SubscriptionPlan) => {
    if (plan === currentPlan) return;
    
    setIsProcessing(true);
    setError(null);
    
    try {
      // TODO: Implement Stripe checkout or billing portal redirect
      // For now, show a message
      alert(`La funcionalidad de cambio de plan estará disponible próximamente. Plan seleccionado: ${PLAN_DETAILS[plan].name}`);
      
      // In production, this would:
      // 1. Open Stripe Checkout for upgrades
      // 2. Open Stripe Customer Portal for downgrades
      // 3. Update user's plan in the backend
      // 4. Refresh usage stats
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Error al procesar el cambio de plan');
    } finally {
      setIsProcessing(false);
    }
  }, [currentPlan]);
  
  // Handle invoice download
  const handleInvoiceDownload = useCallback(async (invoice: Invoice) => {
    if (!invoice.pdfUrl) return;
    
    try {
      // Open PDF in new tab/window (browser will handle download)
      window.open(invoice.pdfUrl, '_blank');
    } catch (err) {
      setError('Error al descargar la factura');
    }
  }, []);
  
  // Determine which plans are upgrades/downgrades
  const planOrder: SubscriptionPlan[] = ['byok_free', 'starter', 'pro'];
  const currentPlanIndex = planOrder.indexOf(currentPlan);
  
  const isUpgrade = (plan: SubscriptionPlan) => planOrder.indexOf(plan) > currentPlanIndex;
  const isDowngrade = (plan: SubscriptionPlan) => planOrder.indexOf(plan) < currentPlanIndex;
  
  if (!isVisible) return null;
  
  const loading = authLoading || usageLoading || loadingInvoices;
  
  return (
    <div className="min-h-screen bg-background">
      {/* Header */}
      <header className="bg-surface border-b border-border sticky top-0 z-10">
        <div className="max-w-5xl mx-auto px-4 py-4 flex items-center gap-4">
          {onBack && (
            <button
              onClick={onBack}
              className="p-2 text-text-secondary hover:text-text hover:bg-surface-hover rounded-lg transition"
            >
              <svg className="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
            </button>
          )}
          <h1 className="text-xl font-semibold text-text">Suscripción</h1>
        </div>
      </header>
      
      {/* Content */}
      <main className="max-w-5xl mx-auto px-4 py-8">
        {/* Error message */}
        {error && (
          <div className="mb-6 p-4 bg-error/10 border border-error/30 rounded-lg">
            <p className="text-error">{error}</p>
            <button 
              onClick={() => setError(null)}
              className="text-sm text-error/70 hover:text-error mt-2"
            >
              Cerrar
            </button>
          </div>
        )}
        
        {/* BYOK Unlimited Banner */}
        {isByokMode && <ByokUnlimitedBanner />}
        
        {/* Usage Summary (only for paid plans) */}
        {!isByokMode && stats && stats.currentMonth.limit > 0 && (
          <UsageSummary
            used={stats.currentMonth.used}
            limit={stats.currentMonth.limit}
            percentage={stats.currentMonth.percentage}
            renewalDate={stats.renewalDate}
          />
        )}
        
        {/* Plan Comparison */}
        <section className="mb-8">
          <h2 className="text-lg font-semibold text-text mb-4">Planes disponibles</h2>
          
          {loading ? (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              {[1, 2, 3].map((i) => (
                <div key={i} className="h-80 bg-surface-hover animate-pulse rounded-xl" />
              ))}
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              {(Object.entries(PLAN_DETAILS) as [SubscriptionPlan, PlanDetails][]).map(([plan, details]) => (
                <PlanCard
                  key={plan}
                  plan={plan}
                  details={details}
                  isCurrentPlan={plan === currentPlan}
                  isByokMode={isByokMode}
                  isUpgrade={isUpgrade(plan)}
                  isDowngrade={isDowngrade(plan)}
                  onSelect={() => handlePlanSelect(plan)}
                  disabled={isProcessing}
                />
              ))}
            </div>
          )}
          
          <p className="mt-4 text-sm text-text-secondary text-center">
            Los cambios de plan se aplican al inicio del próximo ciclo de facturación.
          </p>
        </section>
        
        {/* Invoice History (only for paid plans) */}
        {!isByokMode && (
          <section>
            <h2 className="text-lg font-semibold text-text mb-4">Historial de facturas</h2>
            
            {loadingInvoices ? (
              <div className="space-y-3">
                {[1, 2, 3].map((i) => (
                  <div key={i} className="h-16 bg-surface-hover animate-pulse rounded-lg" />
                ))}
              </div>
            ) : invoices.length > 0 ? (
              <div className="bg-surface border border-border rounded-xl overflow-hidden">
                <div className="divide-y divide-border">
                  {invoices.slice(0, 24).map((invoice) => (
                    <InvoiceRow
                      key={invoice.id}
                      invoice={invoice}
                      onDownload={() => handleInvoiceDownload(invoice)}
                    />
                  ))}
                </div>
              </div>
            ) : (
              <div className="bg-surface border border-border rounded-xl p-8 text-center">
                <svg className="w-12 h-12 text-text-secondary mx-auto mb-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                </svg>
                <p className="text-text-secondary">No hay facturas disponibles</p>
              </div>
            )}
          </section>
        )}
        
        {/* BYOK Info section (for BYOK users) */}
        {isByokMode && (
          <section className="bg-surface border border-border rounded-xl p-6">
            <h2 className="text-lg font-semibold text-text mb-4">Información de facturación</h2>
            <p className="text-text-secondary mb-4">
              Como usuario de BYOK (Bring Your Own Key), tus costos de uso de la API de Gemini 
              se facturan directamente a través de tu cuenta de Google Cloud.
            </p>
            <div className="flex gap-4">
              <a 
                href="https://console.cloud.google.com/billing" 
                target="_blank" 
                rel="noopener noreferrer"
                className="inline-flex items-center gap-2 px-4 py-2 bg-surface-hover text-text hover:text-primary rounded-lg transition"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
                Ver facturación de Google Cloud
              </a>
            </div>
          </section>
        )}
      </main>
    </div>
  );
}

export default SubscriptionPage;
