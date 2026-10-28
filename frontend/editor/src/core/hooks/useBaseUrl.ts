import { useAppConfig } from "@app/contexts/AppConfigContext";

export const useBaseUrl = (): string => {
  const { config } = useAppConfig();
  if (config?.baseUrl) return config.baseUrl;
  // No configured public origin: stay on this app. The previous fallback was
  // the upstream demo site, which would publish 纸环 pages as that product.
  if (typeof window !== "undefined" && window.location?.origin) {
    return window.location.origin;
  }
  return "";
};
