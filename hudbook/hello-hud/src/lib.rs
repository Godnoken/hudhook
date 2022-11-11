use std::time::Instant;


use hudhook::hooks::{ImguiRenderLoop, ImguiRenderLoopFlags};
use hudhook::hooks::dx12::ImguiDX12Hooks;
use imgui::*;

struct HelloHud {
    start_time: Instant,
}

impl HelloHud {
    fn new() -> Self {
        Self { start_time: Instant::now() }
    }
}

impl ImguiRenderLoop for HelloHud {
    fn render(&mut self, ui: &mut Ui, _flags: &ImguiRenderLoopFlags) {
        Window::new("##hello")
            .size([320., 200.], Condition::Always)
            .build(ui, || {
                ui.text("Hello, world!");
                ui.text(format!("Elapsed: {:?}", self.start_time.elapsed()));
            });
    }
}

hudhook::hudhook!(HelloHud::new().into_hook::<ImguiDX12Hooks>());
