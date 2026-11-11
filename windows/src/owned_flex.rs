//! UI-local, SDK-composed variable-size wrapping Panel.
//! Margin/padding belong to the renderer's existing native wrappers. DesiredSize
//! already includes child margins; Arrange receives that same outer slot.
use crate::{
    driver::{callback, sdk_error},
    owned_bindings as n,
};
use n::{IFrameworkElementOverrides, IFrameworkElementOverrides_Impl};
use std::{
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};
use windows_core::{Interface, Result};

pub(super) fn panel(gap: f32) -> Result<n::Panel> {
    n::Panel::compose(FlexPanel {
        gap,
        ui_local: PhantomData,
    })
}

#[windows_core::implement(IFrameworkElementOverrides, Agile = false)]
struct FlexPanel {
    // Layout is immutable on a retained Node; no gap subscription is needed.
    gap: f32,
    ui_local: PhantomData<Rc<()>>,
}

impl FlexPanel_Impl {
    fn base_interface<T: Interface>(&self) -> Result<T> {
        // The official implement macro owns the non-delegating inner. Query it
        // only for this call: retaining a delegating interface here would cycle.
        self.base
            .as_option()
            .as_ref()
            .ok_or_else(|| sdk_error("Flex Panel base not composed"))?
            .cast()
    }

    fn layout(&self, width: f32, measure: bool) -> Result<(Vec<n::UIElement>, Flow)> {
        let list = self.base_interface::<n::IPanel>()?.Children()?;
        let count = list.Size()? as usize;
        if count > super::MAX_NODES {
            return Err(sdk_error("Flex child budget"));
        }
        let mut children = Vec::with_capacity(count);
        let mut sizes = Vec::with_capacity(count);
        for index in 0..count {
            let child = list.GetAt(index as u32)?;
            let visible = child.Visibility()? != n::Visibility::Collapsed;
            if measure {
                child.Measure(n::Size {
                    width,
                    height: f32::INFINITY,
                })?;
            }
            sizes.push(if visible {
                Some(child.DesiredSize()?)
            } else {
                None
            });
            children.push(child);
        }
        let flow = flow(width, self.gap, &sizes).map_err(sdk_error)?;
        Ok((children, flow))
    }
}

// Never unwind or throw an unhandled XAML layout failure through the native
// override. Preserve the fault in the driver's deferred callback teardown path.
fn contained<T>(fallback: T, work: impl FnOnce() -> Result<T>) -> Result<T> {
    let error = match catch_unwind(AssertUnwindSafe(work)) {
        Ok(Ok(value)) => return Ok(value),
        Ok(Err(error)) => error,
        Err(panic) => sdk_error(format!(
            "Flex layout panic: {}",
            panechord_runtime::panic_message(&*panic)
        )),
    };
    callback(|| Err(error));
    Ok(fallback)
}

#[allow(non_snake_case)]
impl IFrameworkElementOverrides_Impl for FlexPanel_Impl {
    fn MeasureOverride(&self, available: &n::Size) -> Result<n::Size> {
        contained(n::Size::default(), || {
            Ok(self.layout(available.width, true)?.1.extent)
        })
    }

    fn ArrangeOverride(&self, final_size: &n::Size) -> Result<n::Size> {
        contained(*final_size, || {
            let (children, flow) = self.layout(final_size.width, false)?;
            for (child, slot) in children.iter().zip(flow.slots) {
                child.Arrange(slot.unwrap_or_default())?;
            }
            Ok(*final_size)
        })
    }

    fn OnApplyTemplate(&self) -> Result<()> {
        contained((), || {
            self.base_interface::<IFrameworkElementOverrides>()?
                .OnApplyTemplate()
        })
    }

    fn GoToElementStateCore(
        &self,
        name: &windows_core::HSTRING,
        transitions: bool,
    ) -> Result<bool> {
        contained(false, || {
            self.base_interface::<IFrameworkElementOverrides>()?
                .GoToElementStateCore(&name.to_string_lossy(), transitions)
        })
    }
}

struct Flow {
    slots: Vec<Option<n::Rect>>,
    extent: n::Size,
}

// No native calls: finite geometry, explicit occupancy (even zero-width children
// consume a slot), no leading/trailing gap, and oversized first items stay first.
fn flow(
    width: f32,
    gap: f32,
    sizes: &[Option<n::Size>],
) -> std::result::Result<Flow, &'static str> {
    if sizes.len() > super::MAX_NODES {
        return Err("Flex child budget");
    }
    if width.is_nan() || width < 0.0 || !gap.is_finite() || gap < 0.0 {
        return Err("invalid Flex width/gap");
    }
    let (mut x, mut y, mut line, mut extent): (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0);
    let mut occupied = false;
    let mut slots = Vec::with_capacity(sizes.len());
    for size in sizes {
        let Some(size) = size else {
            slots.push(None);
            continue;
        };
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width < 0.0
            || size.height < 0.0
        {
            return Err("invalid Flex desired size");
        }
        let (w, h) = (f64::from(size.width), f64::from(size.height));
        let mut start = if occupied { x + f64::from(gap) } else { 0.0 };
        if occupied && start + w > f64::from(width) {
            start = 0.0;
            y += line + f64::from(gap);
            line = 0.0;
        }
        x = start + w;
        line = line.max(h);
        extent = extent.max(x);
        if x > f64::from(f32::MAX) || y + line > f64::from(f32::MAX) {
            return Err("Flex geometry overflow");
        }
        slots.push(Some(n::Rect {
            x: start as f32,
            y: y as f32,
            width: size.width,
            height: size.height,
        }));
        occupied = true;
    }
    Ok(Flow {
        slots,
        extent: n::Size {
            width: extent as f32,
            height: (y + line) as f32,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn size(width: f32, height: f32) -> Option<n::Size> {
        Some(n::Size { width, height })
    }
    fn position(flow: &Flow, index: usize) -> (f32, f32) {
        let r = flow.slots[index].unwrap();
        (r.x, r.y)
    }
    #[test]
    fn variable_widths_wrap_without_column_tracks_or_trailing_gaps() {
        let f = flow(
            100.0,
            5.0,
            &[
                size(60.0, 10.0),
                size(30.0, 20.0),
                size(25.0, 7.0),
                size(65.0, 8.0),
            ],
        )
        .unwrap();
        assert_eq!(position(&f, 0), (0.0, 0.0));
        assert_eq!(position(&f, 1), (65.0, 0.0));
        assert_eq!(position(&f, 2), (0.0, 25.0));
        assert_eq!(position(&f, 3), (30.0, 25.0));
        assert_eq!(
            f.extent,
            n::Size {
                width: 95.0,
                height: 33.0
            }
        );
    }
    #[test]
    fn oversized_first_does_not_add_an_empty_line() {
        let f = flow(40.0, 3.0, &[size(70.0, 10.0), size(20.0, 8.0)]).unwrap();
        assert_eq!(position(&f, 0), (0.0, 0.0));
        assert_eq!(position(&f, 1), (0.0, 13.0));
        assert_eq!(
            f.extent,
            n::Size {
                width: 70.0,
                height: 21.0
            }
        );
    }
    #[test]
    fn collapsed_children_consume_neither_space_nor_gap() {
        let f = flow(
            100.0,
            5.0,
            &[None, size(40.0, 10.0), None, size(55.0, 20.0), None],
        )
        .unwrap();
        assert_eq!(position(&f, 3), (45.0, 0.0));
        assert_eq!(f.slots[2], None);
        assert_eq!(
            f.extent,
            n::Size {
                width: 100.0,
                height: 20.0
            }
        );
        assert_eq!(flow(0.0, 5.0, &[None]).unwrap().extent, n::Size::default());
    }
    #[test]
    fn unbounded_empty_zero_and_exact_fit() {
        let sizes = [size(30.0, 10.0), size(65.0, 20.0)];
        assert_eq!(
            position(&flow(f32::INFINITY, 5.0, &sizes).unwrap(), 1),
            (35.0, 0.0)
        );
        assert_eq!(position(&flow(100.0, 5.0, &sizes).unwrap(), 1), (35.0, 0.0));
        assert_eq!(flow(10.0, 9.0, &[]).unwrap().extent, n::Size::default());
        let f = flow(0.0, 5.0, &[size(0.0, 0.0), size(0.0, 0.0)]).unwrap();
        assert_eq!(position(&f, 1), (0.0, 5.0));
        assert_eq!(f.extent.height, 5.0);
    }
    #[test]
    fn resizing_and_child_size_changes_reflow_from_current_measurements() {
        let mut sizes = [size(60.0, 10.0), size(30.0, 20.0)];
        assert_eq!(position(&flow(90.0, 5.0, &sizes).unwrap(), 1), (0.0, 15.0));
        assert_eq!(position(&flow(95.0, 5.0, &sizes).unwrap(), 1), (65.0, 0.0));
        sizes[0] = size(70.0, 10.0);
        assert_eq!(position(&flow(95.0, 5.0, &sizes).unwrap(), 1), (0.0, 15.0));
    }
    #[test]
    fn budgets_and_invalid_geometry_fail_before_allocation_or_overflow() {
        assert!(flow(1.0, 0.0, &vec![None; super::super::MAX_NODES + 1]).is_err());
        assert_eq!(
            flow(1.0, 0.0, &vec![None; super::super::MAX_NODES])
                .unwrap()
                .slots
                .len(),
            super::super::MAX_NODES
        );
        assert!(flow(f32::NAN, 0.0, &[]).is_err());
        assert!(flow(-1.0, 0.0, &[]).is_err());
        assert!(flow(10.0, -1.0, &[]).is_err());
        assert!(flow(10.0, f32::INFINITY, &[]).is_err());
        assert!(flow(10.0, 0.0, &[size(f32::NAN, 1.0)]).is_err());
        assert!(
            flow(
                f32::INFINITY,
                0.0,
                &[size(f32::MAX, 1.0), size(f32::MAX, 1.0)]
            )
            .is_err()
        );
    }
}
