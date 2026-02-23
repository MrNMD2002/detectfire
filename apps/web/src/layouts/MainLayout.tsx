import { Outlet, NavLink } from 'react-router-dom';
import { 
  LayoutDashboard, 
  Camera, 
  Bell, 
  Settings, 
  LogOut,
  Flame
} from 'lucide-react';
import { useAuthStore } from '@/stores/authStore';

const navItems = [
  { to: '/dashboard', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/cameras', icon: Camera, label: 'Cameras' },
  { to: '/events', icon: Bell, label: 'Events' },
  { to: '/settings', icon: Settings, label: 'Settings' },
];

export default function MainLayout() {
  const { user, logout } = useAuthStore();

  return (
    <div className="app-layout">
      {/* Sidebar */}
      <aside className="sidebar">
        <div className="sidebar-header">
          <div className="sidebar-logo">
            <div className="sidebar-logo-icon">
              <Flame size={24} />
            </div>
            <span className="sidebar-logo-text">Fire Detect</span>
          </div>
        </div>

        <nav className="sidebar-nav">
          {navItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              className={({ isActive }) =>
                `nav-item ${isActive ? 'active' : ''}`
              }
            >
              <item.icon className="nav-item-icon" size={20} />
              <span>{item.label}</span>
            </NavLink>
          ))}
        </nav>

        <div style={{ padding: 'var(--spacing-4)', borderTop: '1px solid var(--border-color)' }}>
          <div style={{ marginBottom: 'var(--spacing-4)' }}>
            <div style={{ fontSize: '0.875rem', fontWeight: 500, color: 'var(--text-primary)' }}>
              {user?.name}
            </div>
            <div style={{ fontSize: '0.75rem', color: 'var(--text-secondary)' }}>
              {user?.email}
            </div>
          </div>
          <button
            onClick={logout}
            className="nav-item"
            style={{ width: '100%', cursor: 'pointer', background: 'none', border: 'none' }}
          >
            <LogOut size={20} />
            <span>Đăng xuất</span>
          </button>
        </div>
      </aside>

      {/* Main Content */}
      <main className="main-content">
        <Outlet />
      </main>
    </div>
  );
}
