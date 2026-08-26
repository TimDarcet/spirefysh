using System;
using System.Collections;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using HarmonyLib;
using MegaCrit.Sts2.Core.CardSelection;
using MegaCrit.Sts2.Core.Commands;
using MegaCrit.Sts2.Core.Entities.CardRewardAlternatives;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.Entities.Merchant;
using MegaCrit.Sts2.Core.Entities.Players;
using MegaCrit.Sts2.Core.Entities.RestSite;
using MegaCrit.Sts2.Core.GameActions;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Multiplayer.Game;
using MegaCrit.Sts2.Core.Nodes.Screens.CardSelection;
using MegaCrit.Sts2.Core.Rewards;
using MegaCrit.Sts2.Core.Runs;
using LiveCardSelector = MegaCrit.Sts2.Core.Nodes.Screens.CardSelection.ICardSelector;

namespace Spirefysh.Bridge;

/// <summary>Read-only observation patches for non-combat and nested decision boundaries.</summary>
internal static class BridgePatches
{
    private static TraceCapture? _capture;
    private static RewardsSet? _rewardsSet;
    private static CardReward? _cardReward;

    internal static void Install(Harmony harmony, TraceCapture capture)
    {
        _capture = capture;
        var liveSelector = typeof(LiveCardSelector);
        _ = AccessTools.Method(liveSelector, nameof(LiveCardSelector.CardsSelected))
            ?? throw new MissingMethodException(liveSelector.FullName,
                nameof(LiveCardSelector.CardsSelected));
        PatchPrefix(harmony, typeof(PlayerChoiceSynchronizer),
            nameof(PlayerChoiceSynchronizer.SyncLocalChoice), nameof(BeforePlayerChoice));
        PatchPrefix(harmony, typeof(RewardsSetSynchronizer),
            nameof(RewardsSetSynchronizer.BeginRewardsSet), nameof(BeforeRewardsSet));
        PatchPrefix(harmony, typeof(RewardsSetSynchronizer),
            nameof(RewardsSetSynchronizer.SelectLocalReward), nameof(BeforeRewardSelected));
        PatchPrefix(harmony, typeof(RewardsSetSynchronizer),
            nameof(RewardsSetSynchronizer.SkipLocalRewardsSet), nameof(BeforeRewardsSkipped));
        PatchPrefix(harmony, typeof(RestSiteSynchronizer),
            nameof(RestSiteSynchronizer.ChooseLocalOption), nameof(BeforeRestSiteOption));
        PatchPrefix(harmony, typeof(EventSynchronizer),
            nameof(EventSynchronizer.ChooseLocalOption), nameof(BeforeEventOption));
        var cardReward = AccessTools.Method(typeof(CardReward), "OnSelect")
            ?? throw new MissingMethodException(typeof(CardReward).FullName, "OnSelect");
        harmony.Patch(cardReward,
            prefix: new HarmonyMethod(typeof(BridgePatches), nameof(BeforeCardReward)));
        PatchPrefix(harmony, typeof(NCardRewardSelectionScreen),
            nameof(NCardRewardSelectionScreen.ShowScreen), nameof(BeforeCardRewardOptions));
        PatchPrefix(harmony, typeof(NCardRewardSelectionScreen),
            nameof(NCardRewardSelectionScreen.RefreshOptions), nameof(BeforeCardRewardOptions));

        foreach (var method in typeof(CardSelectCmd).GetMethods(
                     BindingFlags.Public | BindingFlags.Static | BindingFlags.DeclaredOnly)
                     .Where(method => method.Name == nameof(CardSelectCmd.FromChooseABundleScreen) ||
                                      method.GetParameters().Any(parameter =>
                                          typeof(IEnumerable<CardModel>).IsAssignableFrom(parameter.ParameterType) ||
                                          parameter.ParameterType == typeof(CardPile) ||
                                          parameter.ParameterType == typeof(Func<CardModel, bool>))))
            harmony.Patch(method,
                prefix: new HarmonyMethod(typeof(BridgePatches), nameof(BeforeCardSelection)));
        foreach (var method in liveSelector.Assembly.GetTypes()
                     .Where(type => !type.IsAbstract && liveSelector.IsAssignableFrom(type))
                     .SelectMany(type => type.GetMethods(BindingFlags.Public | BindingFlags.Instance |
                                                         BindingFlags.Static | BindingFlags.DeclaredOnly))
                     .Where(method => method.Name is "Create" or "ShowScreen")
                     .Where(method => method.GetParameters().Any(parameter =>
                         typeof(IEnumerable<CardModel>).IsAssignableFrom(parameter.ParameterType) ||
                         parameter.ParameterType == typeof(CardPile))))
            harmony.Patch(method,
                prefix: new HarmonyMethod(typeof(BridgePatches), nameof(BeforeCardSelection)));
        var runEnded = AccessTools.Method(typeof(RunManager), nameof(RunManager.OnEnded))
            ?? throw new MissingMethodException(typeof(RunManager).FullName, nameof(RunManager.OnEnded));
        harmony.Patch(runEnded,
            postfix: new HarmonyMethod(typeof(BridgePatches), nameof(AfterRunEnded)));
        PatchPrefix(harmony, typeof(RunManager),
            nameof(RunManager.Abandon), nameof(BeforeRunAbandoned));

        foreach (var method in typeof(MerchantEntry).Assembly.GetTypes()
                     .Where(type => typeof(MerchantEntry).IsAssignableFrom(type))
                     .SelectMany(type => type.GetMethods(
                         BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly))
                     .Where(method => method.Name == "OnTryPurchaseWrapper"))
            harmony.Patch(method,
                prefix: new HarmonyMethod(typeof(BridgePatches), nameof(BeforeMerchantPurchase)));

    }

    private static void PatchPrefix(
        Harmony harmony,
        Type type,
        string target,
        string prefix)
    {
        var method = AccessTools.Method(type, target)
            ?? throw new MissingMethodException(type.FullName, target);
        harmony.Patch(method,
            prefix: new HarmonyMethod(typeof(BridgePatches), prefix));
    }

    private static void BeforeCardSelection(MethodBase __originalMethod, object[] __args) =>
        Observe("card selection offer", () =>
    {
        if (__originalMethod.Name == nameof(CardSelectCmd.FromChooseABundleScreen))
            throw new NotSupportedException("Card-bundle choices require a typed bundle descriptor.");
        var prefs = __args.OfType<CardSelectorPrefs>()
            .Select(value => (CardSelectorPrefs?)value).SingleOrDefault();
        var filter = __args.OfType<Func<CardModel, bool>>().SingleOrDefault();
        var options = __args.OfType<IEnumerable<CardModel>>().FirstOrDefault()
            ?? __args.OfType<CardPile>().SingleOrDefault()?.Cards
            ?? (__originalMethod.Name.StartsWith("FromDeck", StringComparison.Ordinal)
                ? __args.OfType<Player>().SingleOrDefault()?.Deck.Cards
                : __args.OfType<Player>().SingleOrDefault()?.PlayerCombatState?.Hand.Cards)
            ?? throw new InvalidOperationException("Pinned card-selection screen did not expose its cards.");
        var candidates = options.Where(card => filter?.Invoke(card) ?? true).ToArray();
        var canSkip = prefs?.Cancelable == true || prefs?.MinSelect == 0 ||
                      prefs is null && __args.OfType<bool>().SingleOrDefault();
        OfferCards(candidates, prefs?.MinSelect ?? (canSkip ? 0 : 1), prefs?.MaxSelect ?? 1,
            canSkip);
    });

    private static void BeforeCardReward(CardReward __instance) =>
        Observe("card reward offer", () => _cardReward = __instance);

    private static void BeforeCardRewardOptions(object[] __args) =>
        Observe("card reward alternatives", () =>
        {
            var alternatives = __args.OfType<IReadOnlyList<CardRewardAlternative>>().Single();
            var reward = _cardReward ?? throw new InvalidOperationException(
                "Card reward options appeared without an active CardReward.");
            var cards = __args.OfType<IReadOnlyList<CardCreationResult>>().Single()
                .Select(result => result.Card);
            _capture?.OnCardChoiceOffered(cards, reward.CanSkip ? 0 : 1, 1, reward.CanSkip,
                alternatives.Select(option => option.OptionId).ToArray());
        });

    private static void OfferCards(IEnumerable<CardModel> options, int minimum, int maximum, bool canSkip)
        => _capture?.OnCardChoiceOffered(options, minimum, maximum, canSkip);

    private static void BeforePlayerChoice(object[] __args) => Observe("player choice", () =>
    {
        var result = __args.OfType<PlayerChoiceResult>().Single();
        var choiceId = __args.OfType<uint>().Single();
        if (result.ChoiceType == MegaCrit.Sts2.Core.Entities.Models.PlayerChoiceType.None) return;
        if (_capture?.HasChoiceOffer != true)
            throw new InvalidOperationException("Typed player choice had no captured legal offer.");
        _capture?.OnPlayerChoice(result, choiceId);
    });

    private static void BeforeRewardsSet(object[] __args) => Observe("rewards offered", () =>
    {
        _rewardsSet = __args.OfType<RewardsSet>().Single();
        _capture?.OnRewardsSetBegun(_rewardsSet);
    });

    private static void BeforeRewardSelected(object[] __args) => Observe("reward selected", () =>
    {
        var reward = __args.OfType<Reward>().Single();
        var set = _rewardsSet ?? throw new InvalidOperationException(
            "Reward selection occurred without a captured RewardsSet.");
        var legal = RewardActions(set);
        var index = set.Rewards.FindIndex(candidate => ReferenceEquals(candidate, reward));
        if (index < 0) throw new InvalidOperationException("Selected reward is not in the visible set.");
        _capture?.BeginExternalDecision(
            reward,
            set.Id,
            RewardAction(set, reward, index),
            legal);
    });

    private static void BeforeRewardsSkipped() => Observe("rewards skipped", () =>
    {
        var set = _rewardsSet ?? throw new InvalidOperationException(
            "Reward skip occurred without a captured RewardsSet.");
        var skip = new SemanticAction(
            "skip_rewards", $"skip-rewards:{set.Id}", null, null,
            nameof(RewardsSetSynchronizer));
        _capture?.BeginExternalDecision(set, set.Id, skip, RewardActions(set));
    });

    private static SemanticAction[] RewardActions(RewardsSet set)
    {
        var actions = set.Rewards.Select((reward, index) =>
                RewardAction(set, reward, index))
            .ToList();
        if (!set.DisallowSkipping)
            actions.Add(new(
                "skip_rewards", $"skip-rewards:{set.Id}", null, null,
                nameof(RewardsSetSynchronizer)));
        return actions.ToArray();
    }

    private static SemanticAction RewardAction(RewardsSet set, Reward reward, int index) =>
        new(
            "take_reward",
            $"reward:{set.Id}:{index}:{reward.GetType().Name}",
            $"reward:{set.Id}:{index}",
            null,
            reward.GetType().FullName ?? reward.GetType().Name);

    private static void BeforeRestSiteOption(RestSiteSynchronizer __instance, int index) =>
        Observe("rest-site option", () =>
    {
        var options = __instance.GetLocalOptions();
        var legal = options.Select((option, optionIndex) =>
                RestAction(option, optionIndex))
            .Where((_, optionIndex) => options[optionIndex].IsEnabled)
            .ToArray();
        if (index < 0 || index >= options.Count || !options[index].IsEnabled)
            throw new InvalidOperationException("Selected rest-site option is not enabled.");
        _capture?.BeginExternalDecision(
            options[index], index, RestAction(options[index], index), legal);
    });

    private static SemanticAction RestAction(RestSiteOption option, int index) =>
        new(
            "rest_site",
            $"rest:{index}:{option.OptionId}",
            option.OptionId,
            null,
            option.GetType().FullName ?? option.GetType().Name);

    private static void BeforeEventOption(EventSynchronizer __instance, int index) =>
        Observe("event option", () =>
    {
        var eventModel = __instance.GetLocalEvent();
        var property = eventModel.GetType().GetProperty(
            "CurrentOptions", BindingFlags.Public | BindingFlags.Instance)
            ?? throw new MissingMemberException(eventModel.GetType().FullName, "CurrentOptions");
        var options = ((IEnumerable)(property.GetValue(eventModel)
            ?? throw new InvalidOperationException("Visible event options are unavailable.")))
            .Cast<object>()
            .ToArray();
        var legal = options.Select((option, optionIndex) =>
                EventAction(eventModel, option, optionIndex))
            .Where((_, optionIndex) => !ReadBool(options[optionIndex], "IsLocked"))
            .ToArray();
        if (index < 0 || index >= options.Length || ReadBool(options[index], "IsLocked"))
            throw new InvalidOperationException("Selected event option is not unlocked.");
        _capture?.BeginExternalDecision(
            options[index], index, EventAction(eventModel, options[index], index), legal);
    });

    private static SemanticAction EventAction(
        EventModel eventModel,
        object option,
        int index)
    {
        var textKey = ReadString(option, "TextKey");
        return new(
            "event_option",
            $"event:{eventModel.Id}:{index}:{textKey}",
            $"event:{eventModel.Id}",
            null,
            option.GetType().FullName ?? option.GetType().Name);
    }

    private static void BeforeMerchantPurchase(MerchantEntry __instance, object[] __args) =>
        Observe("merchant purchase", () =>
    {
        var inventory = __args.OfType<MerchantInventory>().Single();
        var entries = inventory.AllEntries.ToArray();
        var legal = entries.Select((entry, index) => MerchantAction(entry, index))
            .Where((_, index) => entries[index].IsStocked && entries[index].EnoughGold)
            .ToArray();
        var selected = Array.FindIndex(entries, candidate => ReferenceEquals(candidate, __instance));
        if (selected < 0 || !__instance.IsStocked || !__instance.EnoughGold)
            throw new InvalidOperationException("Selected merchant entry is not legally purchasable.");
        _capture?.BeginExternalDecision(
            __instance,
            selected,
            MerchantAction(__instance, selected),
            legal);
    });

    private static SemanticAction MerchantAction(MerchantEntry entry, int index)
    {
        var modelId = entry switch
        {
            MerchantCardEntry card => card.CreationResult?.Card?.Id.ToString() ?? "empty-card",
            MerchantRelicEntry relic => relic.Model?.Id.ToString() ?? "empty-relic",
            MerchantPotionEntry potion => potion.Model?.Id.ToString() ?? "empty-potion",
            MerchantCardRemovalEntry => "card-removal",
            _ => entry.GetType().Name
        };
        return new(
            "shop_purchase",
            $"shop:{index}:{modelId}:{entry.Cost}",
            modelId,
            null,
            entry.GetType().FullName ?? entry.GetType().Name);
    }

    private static void AfterRunEnded(bool isVictory) => Observe("run ended", () =>
        _capture?.CompleteEpisode(isVictory, isVictory ? "won" : "lost"));

    private static void BeforeRunAbandoned() => Observe("run abandoned", () =>
        _capture?.CompleteEpisode(null, "abandoned"));

    private static void Observe(string context, Action observation)
    {
        try { observation(); }
        catch (Exception exception) { _capture?.CaptureGap(context, exception); }
    }

    private static bool ReadBool(object instance, string property) =>
        (bool)(instance.GetType().GetProperty(property, BindingFlags.Public | BindingFlags.Instance)
            ?.GetValue(instance)
            ?? throw new MissingMemberException(instance.GetType().FullName, property));

    private static string ReadString(object instance, string property) =>
        (string)(instance.GetType().GetProperty(property, BindingFlags.Public | BindingFlags.Instance)
            ?.GetValue(instance)
            ?? throw new MissingMemberException(instance.GetType().FullName, property));
}
