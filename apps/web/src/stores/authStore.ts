import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { api, authApi } from '@/lib/api';

interface User {
  id: string;
  email: string;
  name: string;
  role: string;
}

interface AuthState {
  token: string | null;
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  error: string | null;
  
  login: (email: string, password: string) => Promise<void>;
  logout: () => void;
  refreshUser: () => Promise<void>;
}

// NOTE: The most secure token storage for a web SPA is an HttpOnly, Secure,
// SameSite=Strict cookie set by the server (invisible to JavaScript).
// That requires a matching backend change (POST /auth/login sets the cookie
// and GET /auth/me reads it). Until that is implemented, we store the token in
// sessionStorage instead of localStorage:
//   • sessionStorage is cleared when the tab/browser closes → shorter exposure window
//   • Still readable by JS (XSS risk remains), but reduces persistent token theft
//   • TODO: migrate to HttpOnly cookie + remove token from JS-visible storage

export const useAuthStore = create<AuthState>()(
  persist(
    (set, get) => ({
      token: null,
      user: null,
      isAuthenticated: false,
      isLoading: false,
      error: null,

      login: async (email: string, password: string) => {
        set({ isLoading: true, error: null });

        try {
          const data = await authApi.login(email, password);
          const { token, user } = data;

          set({
            token,
            user,
            isAuthenticated: true,
            isLoading: false,
          });

          // Set token in API client
          api.defaults.headers.common['Authorization'] = `Bearer ${token}`;
        } catch (error: any) {
          set({
            isLoading: false,
            error: error.response?.data?.message || 'Đăng nhập thất bại',
          });
          throw error;
        }
      },

      logout: () => {
        set({
          token: null,
          user: null,
          isAuthenticated: false,
        });

        delete api.defaults.headers.common['Authorization'];
      },

      refreshUser: async () => {
        const { token } = get();
        if (!token) return;

        try {
          api.defaults.headers.common['Authorization'] = `Bearer ${token}`;
          const user = await authApi.me();
          set({ user });
        } catch {
          // Token invalid, logout
          get().logout();
        }
      },
    }),
    {
      name: 'auth-storage',
      // Use sessionStorage instead of localStorage to reduce token exposure
      storage: {
        getItem: (key) => {
          const value = sessionStorage.getItem(key);
          return value ? JSON.parse(value) : null;
        },
        setItem: (key, value) => {
          sessionStorage.setItem(key, JSON.stringify(value));
        },
        removeItem: (key) => {
          sessionStorage.removeItem(key);
        },
      },
      partialize: (state) => ({
        token: state.token,
        user: state.user,
        isAuthenticated: state.isAuthenticated,
      }) as AuthState,
    }
  )
);

// Initialize auth on app load
const initAuth = async () => {
  const { token, refreshUser } = useAuthStore.getState();
  if (token) {
    await refreshUser();
  }
};

initAuth();
