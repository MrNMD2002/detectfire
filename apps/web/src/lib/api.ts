import axios from 'axios';

// Create axios instance
export const api = axios.create({
  baseURL: '/api',
  timeout: 10000,
  headers: {
    'Content-Type': 'application/json',
  },
});

// Response interceptor for error handling
api.interceptors.response.use(
  (response) => response,
  (error) => {
    if (error.response?.status === 401) {
      // Token expired, redirect to login
      window.location.href = '/login';
    }
    return Promise.reject(error);
  }
);

// API functions - extract data from response
export const camerasApi = {
  list: async () => {
    const response = await api.get('/cameras');
    return response.data;
  },
  get: async (id: string) => {
    const response = await api.get(`/cameras/${id}`);
    return response.data;
  },
  create: async (data: any) => {
    const response = await api.post('/cameras', data);
    return response.data;
  },
  update: async (id: string, data: any) => {
    const response = await api.put(`/cameras/${id}`, data);
    return response.data;
  },
  delete: async (id: string) => {
    const response = await api.delete(`/cameras/${id}`);
    return response.data;
  },
  getStatus: async (id: string) => {
    const response = await api.get(`/cameras/${id}/status`);
    return response.data;
  },
  getAllStatuses: async (): Promise<Record<string, any>> => {
    const response = await api.get('/cameras/statuses');
    return response.data;
  },
};

export const eventsApi = {
  /** Returns { data: Event[], total: number } */
  list: async (params?: any) => {
    const response = await api.get('/events', { params });
    return response.data as { data: any[]; total: number };
  },
  get: async (id: string) => {
    const response = await api.get(`/events/${id}`);
    return response.data;
  },
  acknowledge: async (id: string) => {
    const response = await api.post(`/events/${id}/acknowledge`);
    return response.data;
  },
  stats: async (params?: any) => {
    const response = await api.get('/events/stats', { params });
    return response.data;
  },
};

export const authApi = {
  login: async (email: string, password: string) => {
    const response = await api.post('/auth/login', { email, password });
    return response.data;
  },
  me: async () => {
    const response = await api.get('/auth/me');
    return response.data;
  },
  refresh: async () => {
    const response = await api.post('/auth/refresh');
    return response.data;
  },
};

export const settingsApi = {
  getTelegram: async () => {
    const response = await api.get('/settings/telegram');
    return response.data;
  },
  updateTelegram: async (data: {
    bot_token?: string;
    default_chat_id: string;
    enabled: boolean;
    rate_limit_per_minute?: number;
  }) => {
    const response = await api.put('/settings/telegram', data);
    return response.data;
  },
  testTelegram: async () => {
    const response = await api.post('/settings/telegram/test');
    return response.data;
  },
};
