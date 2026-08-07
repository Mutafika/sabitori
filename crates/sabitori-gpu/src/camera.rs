//! 3Dオービットカメラ
//!
//! SceneApp用。ドラッグで回転、スクロールでズーム、Shift+ドラッグでパン。

use glam::{Mat4, Vec3};

pub struct OrbitCamera {
    /// 注視点
    pub target: Vec3,
    /// 注視点からの距離
    pub distance: f32,
    /// 水平角（ラジアン、Y軸周り）
    pub yaw: f32,
    /// 垂直角（ラジアン、-π/2〜π/2）
    pub pitch: f32,
    /// 視野角（ラジアン）
    pub fov: f32,
    /// Near clip
    pub near: f32,
    /// Far clip
    pub far: f32,
    /// ズーム速度
    pub zoom_speed: f32,
    /// 回転速度
    pub rotate_speed: f32,
    /// パン速度
    pub pan_speed: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 500.0,
            yaw: 0.0,
            pitch: -0.6,
            fov: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 1_000_000.0,
            zoom_speed: 0.1,
            rotate_speed: 0.005,
            pan_speed: 1.0,
        }
    }
}

impl OrbitCamera {
    /// カメラの世界座標位置
    pub fn eye(&self) -> Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// upベクトル
    pub fn up(&self) -> Vec3 {
        Vec3::Y
    }

    /// ビュー行列
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye(), self.target, self.up())
    }

    /// 射影行列
    pub fn projection_matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov, aspect, self.near, self.far)
    }

    /// VP行列（projection × view）
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        self.projection_matrix(aspect) * self.view_matrix()
    }

    /// マウスドラッグで回転（dx, dyはピクセル差分）
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * self.rotate_speed;
        self.pitch -= dy * self.rotate_speed;
        // 極点制限
        let limit = std::f32::consts::FRAC_PI_2 - 0.01;
        self.pitch = self.pitch.clamp(-limit, limit);
    }

    /// マウスドラッグでパン（カメラのローカル平面上で移動）
    pub fn pan(&mut self, dx: f32, dy: f32) {
        let forward = (self.target - self.eye()).normalize();
        let right = forward.cross(self.up()).normalize();
        let up = right.cross(forward).normalize();

        let scale = self.distance * self.pan_speed * 0.001;
        self.target += right * (-dx * scale) + up * (dy * scale);
    }

    /// スクロールでズーム（delta > 0 で近づく）
    pub fn zoom(&mut self, delta: f32) {
        let factor = (1.0 - delta * self.zoom_speed).clamp(0.5, 2.0);
        self.distance *= factor;
        self.distance = self.distance.clamp(1.0, self.far * 0.5);
    }

    /// カメラを特定の位置にフォーカス（ターゲットと距離を設定）
    pub fn look_at(&mut self, target: Vec3, distance: f32) {
        self.target = target;
        self.distance = distance;
    }
}
