use crate::model::{Entity, ItemStack};

pub fn add_cargo(entity: &mut Entity, item: ItemStack) {
    if let Some(existing) = entity
        .cargo
        .iter_mut()
        .find(|existing| existing.kind == item.kind)
    {
        existing.amount += item.amount;
    } else {
        entity.cargo.push(item);
    }
}

pub fn remove_cargo(entity: &mut Entity, kind: &str, amount: u32) -> u32 {
    let available = entity
        .cargo
        .iter()
        .filter(|stack| stack.kind == kind)
        .map(|stack| stack.amount)
        .sum::<u32>();
    let removed = amount.min(available);
    if removed == 0 {
        return 0;
    }
    let mut remaining = removed;
    entity.cargo.retain_mut(|stack| {
        if stack.kind != kind || remaining == 0 {
            return true;
        }
        let deduct = remaining.min(stack.amount);
        stack.amount -= deduct;
        remaining -= deduct;
        stack.amount > 0
    });

    removed
}
