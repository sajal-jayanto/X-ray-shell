use eframe::egui;
use std::process::Command;

fn main() -> eframe::Result {
  let options = eframe::NativeOptions::default();
  eframe::run_native(
    "X-ray Shell",
    options,
    Box::new(|_cc| Ok(Box::new(ShellApp::default()))),
  )
}

#[derive(Default)]
struct ShellApp {
  input: String,
  output: Vec<String>,
}

impl ShellApp {
  fn run_cmd(&mut self, line: &str) {
    self.output.push(format!("> {line}"));

    let parts: Vec<&str> = line.split_whitespace().collect();
    let Some((cmd, args)) = parts.split_first() else {
      return;
    };

    if *cmd == "clear" {
      self.output.clear();
      return;
    }

    if *cmd == "cd" {
      let dir = args.first().copied().unwrap_or("/");
      if let Err(e) = std::env::set_current_dir(dir) {
        self.output.push(format!("cd: {e}"));
      }
      return;
    }

    match Command::new(cmd).args(args).output() {
      Ok(out) => {
        self
          .output
          .push(String::from_utf8_lossy(&out.stdout).into_owned());
        self
          .output
          .push(String::from_utf8_lossy(&out.stderr).into_owned());
      }
      Err(e) => self.output.push(format!("{cmd}: {e}")),
    }
    self.output.push(format!(
      "---------------------------------------------------------------------------------"
    ));
  }
}

impl eframe::App for ShellApp {
  fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
    let panel_frame = egui::Frame::default().inner_margin(egui::Margin::symmetric(8.0, 12.0));

    egui::TopBottomPanel::bottom("input_panel")
      .frame(panel_frame)
      .show(ctx, |ui| {
        ui.horizontal(|ui| {
          let pwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .map(|s| {
              if let Some(idx) = s.find("/Desktop") {
                s[idx..].to_string()
              } else {
                s
              }
            })
            .expect("Failed to get current directory");

          ui.label(
            egui::RichText::new(format!("~{}$", pwd))
              .font(egui::FontId::monospace(14.0))
              .color(egui::Color32::GREEN),
          );
          let text_edit = egui::TextEdit::singleline(&mut self.input)
            .desired_width(f32::INFINITY)
            .frame(false)
            .font(egui::FontId::monospace(14.0));

          let response = ui.add(text_edit);
          if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let line = std::mem::take(&mut self.input);
            let line = line.trim().to_string();
            if !line.is_empty() {
              self.run_cmd(&line);
            }
          }
          response.request_focus();
        });
      });

    egui::CentralPanel::default().show(ctx, |ui| {
      egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
          egui::Frame::none().show(ui, |ui| {
            for line in &self.output {
              let clean = strip_ansi_escapes::strip(line);
              let line = String::from_utf8_lossy(&clean);
              let line = line.trim_end();
              if !line.is_empty() {
                ui.label(
                  egui::RichText::new(line)
                    .font(egui::FontId::monospace(14.0))
                    .color(egui::Color32::WHITE),
                );
              }
            }
          });
        });
    });
  }
}
