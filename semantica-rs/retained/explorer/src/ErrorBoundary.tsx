import { Component, type ErrorInfo, type ReactNode } from 'react';
import { AlertCircle } from 'lucide-react';

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  retryCount: number;
}

const RETRY_SETTLE_MS = 5000;

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  private settleTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null, retryCount: 0 };
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("ErrorBoundary caught an error:", error, errorInfo);
    this.clearSettleTimer();
  }

  componentWillUnmount() {
    this.clearSettleTimer();
  }

  private clearSettleTimer() {
    if (this.settleTimer !== null) {
      clearTimeout(this.settleTimer);
      this.settleTimer = null;
    }
  }

  resetErrorBoundary = () => {
    this.clearSettleTimer();
    this.setState((prev) => ({
      hasError: false,
      error: null,
      retryCount: prev.retryCount + 1
    }));
    // Only clear the retry count once the workspace has stayed error-free for a
    // sustained period, rather than on the next committed render (which can fire
    // while Suspense is still showing its fallback) or immediately on retry
    // (which would allow an unbounded number of clicks on a deterministic crash).
    this.settleTimer = setTimeout(() => {
      this.settleTimer = null;
      this.setState({ retryCount: 0 });
    }, RETRY_SETTLE_MS);
  };

  render() {
    if (this.state.hasError) {
      const maxRetriesReached = this.state.retryCount >= 3;

      return (
        <div 
          className="workspace-loading" 
          style={{ 
            flexDirection: 'column', 
            gap: 12,
            color: 'var(--ws-red)'
          }}
        >
          <AlertCircle size={32} style={{ marginBottom: 4, opacity: 0.8 }} />
          <div style={{ fontWeight: 500, fontSize: '15px' }}>
            Something went wrong in this view.
          </div>
          <div style={{ fontSize: '13px', opacity: 0.7, maxWidth: 450, textAlign: 'center', marginBottom: 8, lineHeight: 1.5 }}>
            {maxRetriesReached 
              ? "This view continues to encounter a critical error. Please switch to another workspace or reload the page to restore functionality."
              : "An unexpected problem occurred while rendering this workspace. Your data is safe, but this view cannot be displayed."}
          </div>
          {!maxRetriesReached ? (
            <button 
              className="ws-btn ws-btn--ghost" 
              style={{ 
                borderColor: 'var(--ws-red-soft)',
                color: 'var(--ws-red)'
              }}
              onClick={this.resetErrorBoundary}
            >
              Try Again
            </button>
          ) : (
            <button 
              className="ws-btn ws-btn--ghost" 
              style={{ 
                borderColor: 'var(--ws-border)',
                color: 'var(--ws-text)'
              }}
              onClick={() => window.location.reload()}
            >
              Reload Application
            </button>
          )}
        </div>
      );
    }

    return this.props.children;
  }
}
