//! A command owns its process group until every exit path is cleaned up.
use std::process::Child;
pub(crate) struct Running {
    pub child: Child,
    #[cfg(unix)]
    group: i32,
}

impl Running {
    pub fn new(child: Child) -> Self {
        Self {
            #[cfg(unix)]
            group: child.id() as i32,
            child,
        }
    }
    pub fn stop(&mut self) {
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            // Only this command's newly created process group; never the app's group.
            if self.group > 0 {
                unsafe { kill(-self.group, 9) };
                self.group = 0;
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}
