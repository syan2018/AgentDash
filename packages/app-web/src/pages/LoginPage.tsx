import { useCallback, useEffect, useState } from 'react';
import { useAuthStore } from '../stores/authStore';
import type { LoginFieldDescriptor } from '../types';

function LoginField({
  field,
  value,
  onChange,
  disabled,
}: {
  field: LoginFieldDescriptor;
  value: string;
  onChange: (name: string, value: string) => void;
  disabled: boolean;
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label
        htmlFor={`login-${field.name}`}
        className="text-sm font-medium text-foreground"
      >
        {field.label}
        {field.required && <span className="ml-0.5 text-destructive">*</span>}
      </label>
      <input
        id={`login-${field.name}`}
        type={field.field_type === 'password' ? 'password' : 'text'}
        placeholder={field.placeholder ?? undefined}
        required={field.required}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(field.name, e.target.value)}
        className="h-10 rounded-lg border border-input bg-background px-3 text-sm text-foreground outline-none transition-colors placeholder:text-muted-foreground focus:border-ring focus:ring-2 focus:ring-ring/20 disabled:cursor-not-allowed disabled:opacity-50"
      />
    </div>
  );
}

export function LoginPage() {
  const { metadata, isMetadataLoading, fetchMetadata, login, isLoginLoading, loginError } =
    useAuthStore();
  const [formValues, setFormValues] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!metadata) {
      fetchMetadata();
    }
  }, [metadata, fetchMetadata]);

  const handleFieldChange = useCallback((name: string, value: string) => {
    setFormValues((prev) => ({ ...prev, [name]: value }));
  }, []);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      await login({
        username: formValues['username'] ?? '',
        password: formValues['password'] ?? '',
        extra: Object.fromEntries(
          Object.entries(formValues).filter(
            ([k]) => k !== 'username' && k !== 'password',
          ),
        ),
      });
    },
    [formValues, login],
  );

  if (isMetadataLoading) {
    return (
      <div className="flex h-screen items-center justify-center bg-background">
        <div className="text-center">
          <div className="mx-auto h-7 w-7 animate-spin rounded-full border-2 border-primary border-t-transparent" />
          <p className="mt-3 text-sm text-muted-foreground">正在加载认证信息...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-4">
      <div className="w-full max-w-sm">
        <div className="rounded-2xl border border-border bg-card p-8 shadow-sm">
          <div className="mb-6 text-center">
            <h1 className="text-xl font-semibold text-foreground">
              {metadata?.display_name ?? 'AgentDash'}
            </h1>
            {metadata?.description && (
              <p className="mt-1.5 text-sm text-muted-foreground">
                {metadata.description}
              </p>
            )}
          </div>

          <form onSubmit={handleSubmit} className="flex flex-col gap-4">
            {(metadata?.fields ?? []).map((field) => (
              <LoginField
                key={field.name}
                field={field}
                value={formValues[field.name] ?? ''}
                onChange={handleFieldChange}
                disabled={isLoginLoading}
              />
            ))}

            {loginError && (
              <div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-sm text-destructive">
                {loginError}
              </div>
            )}

            <button
              type="submit"
              disabled={isLoginLoading}
              className="mt-2 h-10 rounded-lg bg-primary text-sm font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isLoginLoading ? '登录中...' : '登录'}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
