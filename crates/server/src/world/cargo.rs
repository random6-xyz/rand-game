use crate::model::{Entity, ItemStack};

pub fn add_cargo(entity: &mut Entity, item: ItemStack) -> Result<(), String> {
    if let Some(existing) = entity
        .cargo
        .iter_mut()
        .find(|existing| existing.kind == item.kind)
    {
        existing.amount = existing.amount.checked_add(item.amount).ok_or_else(|| {
            format!(
                "cargo overflow for {}: {} + {}",
                item.kind, existing.amount, item.amount
            )
        })?;
    } else {
        entity.cargo.push(item);
    }
    Ok(())
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
        debug_assert!(deduct <= stack.amount);
        stack.amount -= deduct;
        remaining -= deduct;
        stack.amount > 0
    });

    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Entity;

    fn test_entity() -> Entity {
        Entity {
            id: 1,
            owner_id: 1,
            position: crate::model::Position::new(0, 0),
            cargo: Vec::new(),
        }
    }

    #[test]
    fn add_cargo_returns_err_on_overflow() {
        let mut entity = test_entity();
        let max = ItemStack {
            kind: "iron".into(),
            amount: u32::MAX,
        };
        add_cargo(&mut entity, max).expect("add max should succeed");
        let overflow = ItemStack {
            kind: "iron".into(),
            amount: 1,
        };
        let result = add_cargo(&mut entity, overflow);
        assert!(result.is_err());
    }

    #[test]
    fn add_cargo_merges_same_kind() {
        let mut entity = test_entity();
        let first = ItemStack {
            kind: "iron".into(),
            amount: 10,
        };
        let second = ItemStack {
            kind: "iron".into(),
            amount: 5,
        };
        add_cargo(&mut entity, first).expect("add should succeed");
        add_cargo(&mut entity, second).expect("add should succeed");
        assert_eq!(entity.cargo.len(), 1);
        assert_eq!(entity.cargo[0].amount, 15);
    }

    #[test]
    fn remove_cargo_partial() {
        let mut entity = test_entity();
        entity.cargo = vec![ItemStack {
            kind: "iron".into(),
            amount: 10,
        }];
        let removed = remove_cargo(&mut entity, "iron", 3);
        assert_eq!(removed, 3);
        assert_eq!(entity.cargo.len(), 1);
        assert_eq!(entity.cargo[0].amount, 7);
    }

    #[test]
    fn remove_cargo_exact() {
        let mut entity = test_entity();
        entity.cargo = vec![ItemStack {
            kind: "iron".into(),
            amount: 10,
        }];
        let removed = remove_cargo(&mut entity, "iron", 10);
        assert_eq!(removed, 10);
        assert!(entity.cargo.is_empty());
    }

    #[test]
    fn remove_cargo_more_than_available() {
        let mut entity = test_entity();
        entity.cargo = vec![ItemStack {
            kind: "iron".into(),
            amount: 5,
        }];
        let removed = remove_cargo(&mut entity, "iron", 10);
        assert_eq!(removed, 5);
        assert!(entity.cargo.is_empty());
    }

    #[test]
    fn remove_cargo_empty() {
        let mut entity = test_entity();
        let removed = remove_cargo(&mut entity, "iron", 5);
        assert_eq!(removed, 0);
    }

    #[test]
    fn remove_cargo_from_multiple_stacks() {
        let mut entity = test_entity();
        entity.cargo = vec![
            ItemStack {
                kind: "iron".into(),
                amount: 3,
            },
            ItemStack {
                kind: "copper".into(),
                amount: 2,
            },
            ItemStack {
                kind: "iron".into(),
                amount: 4,
            },
        ];
        let removed = remove_cargo(&mut entity, "iron", 5);
        assert_eq!(removed, 5);
        let iron_stacks: Vec<_> = entity.cargo.iter().filter(|s| s.kind == "iron").collect();
        assert_eq!(iron_stacks.len(), 1);
        assert_eq!(iron_stacks[0].amount, 2);
        assert!(entity.cargo.iter().any(|s| s.kind == "copper"));
    }
}
